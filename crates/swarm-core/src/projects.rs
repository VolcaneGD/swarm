use crate::{
    accounts,
    protocol::Fault,
    protocol::Request,
    service::{required_text, timestamp},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::path::Path;
use uuid::Uuid;

fn directory(path: &str) -> Result<String, Fault> {
    let canonical = Path::new(path).canonicalize().map_err(|_| {
        Fault::new(
            "INVALID_INPUT",
            2,
            "Project path must be an existing accessible directory",
        )
    })?;
    if !canonical.is_dir() {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Project path must be a directory",
        ));
    }
    canonical
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Project path is not valid Unicode"))
}
pub(crate) fn get(db: &Connection, id: &str) -> Result<Value, Fault> {
    let project=db.query_row("SELECT id,name,canonical_root,created_at FROM projects WHERE id=?1",[id],|row| Ok(json!({
        "id":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,"canonical_root":row.get::<_,String>(2)?,"created_at":row.get::<_,String>(3)?
    }))).optional()?.ok_or_else(|| Fault::new("PROJECT_NOT_REGISTERED",5,"Register this project through Admin settings before starting a session"))?;
    let mut query =
        db.prepare("SELECT path FROM project_paths WHERE project_id=?1 ORDER BY path")?;
    let paths = query
        .query_map([id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut project = project;
    project["paths"] = json!(paths);
    Ok(project)
}
fn owner(db: &Connection, path: &str) -> Result<Option<String>, Fault> {
    Ok(db
        .query_row(
            "SELECT project_id FROM project_paths WHERE path=?1",
            [path],
            |row| row.get(0),
        )
        .optional()?)
}
/// Authenticated AI registration never reuses the human route's duplicate-root
/// response: only this actor's exact receipt can reveal the created project.
pub(crate) fn register_ai(db: &Connection, request: &Request, actor: &str) -> Result<Value, Fault> {
    let key = request
        .request_id
        .as_deref()
        .filter(|key| !key.trim().is_empty() && key.len() <= 256)
        .ok_or_else(|| {
            Fault::new(
                "INVALID_INPUT",
                2,
                "Project registration requires a request ID of 1–256 bytes",
            )
        })?;
    let name = required_text(&request.args, "name")?;
    let raw_root = required_text(&request.args, "root")?;
    if name.len() > 512 || raw_root.len() > 32768 {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Project registration input is too large",
        ));
    }
    let request_fingerprint = accounts::digest(
        &serde_json::to_string(
            &json!({"operation":"project.register","args":{"name":name,"root":raw_root}}),
        )
        .map_err(|_| Fault::internal())?,
    );
    let receipt: Option<(String, String, String)> = db.query_row(
        "SELECT fingerprint,operation,entity_id FROM create_requests WHERE actor_id=?1 AND request_id=?2 AND julianday(created_at)>julianday('now','-7 days')",
        params![actor,key], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).optional()?;
    if let Some((prior, operation, id)) = receipt {
        let (prior_request, prior_root) = prior.split_once(':').unwrap_or(("", ""));
        if prior_request != request_fingerprint || operation != "project.register" {
            return Err(Fault::new(
                "REQUEST_CONFLICT",
                6,
                "Request ID was used with different content",
            ));
        }
        let registered = get(db, &id)?;
        let root = registered["canonical_root"]
            .as_str()
            .ok_or_else(Fault::internal)?;
        if accounts::digest(root) != prior_root || owner(db, root)?.as_deref() != Some(&id) {
            return Err(Fault::new(
                "PROJECT_PATH_CONFLICT",
                6,
                "Project registration cannot be completed for this path",
            ));
        }
        return Ok(json!({"id":id,"name":name,"canonical_root":root}));
    }
    let root = directory(raw_root)?;
    let fingerprint = format!("{request_fingerprint}:{}", accounts::digest(&root));
    if owner(db, &root)?.is_some() {
        return Err(Fault::new(
            "PROJECT_PATH_CONFLICT",
            6,
            "Project registration cannot be completed for this path",
        ));
    }
    // Match existing creation-receipt retention without storing path-bearing
    // payloads or responses in the journal.
    db.execute(
        "DELETE FROM create_requests WHERE julianday(created_at)<=julianday('now','-7 days')",
        [],
    )?;
    // Persist exactly the canonical path whose digest is recorded below;
    // do not resolve it again through the human dispatch route.
    let id = Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO projects(id,name,canonical_root,created_at) VALUES(?1,?2,?3,?4)",
        params![id, name, root, timestamp()],
    )?;
    db.execute(
        "INSERT INTO project_paths(path,project_id) VALUES(?1,?2)",
        params![root, id],
    )?;
    db.execute("INSERT INTO create_requests(actor_id,request_id,fingerprint,operation,entity_id,created_at) VALUES(?1,?2,?3,'project.register',?4,?5)",
        params![actor,key,fingerprint,id,timestamp()])?;
    Ok(json!({"id":id,"name":name,"canonical_root":root}))
}
pub(crate) fn dispatch(db: &Connection, operation: &str, args: &Value) -> Result<Value, Fault> {
    match operation {
        "project.register" => {
            let name = required_text(args, "name")?;
            let root = directory(required_text(args, "root")?)?;
            if let Some(id) = owner(db, &root)? {
                return get(db, &id);
            }
            let id = Uuid::new_v4().to_string();
            db.execute(
                "INSERT INTO projects(id,name,canonical_root,created_at) VALUES(?1,?2,?3,?4)",
                params![id, name, root, timestamp()],
            )?;
            db.execute(
                "INSERT INTO project_paths(path,project_id) VALUES(?1,?2)",
                params![root, id],
            )?;
            get(db, &id)
        }
        "project.get" => get(db, required_text(args, "id")?),
        "project.list" => {
            let mut query = db.prepare("SELECT id FROM projects ORDER BY created_at,id")?;
            let ids = query
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({"items":ids.into_iter().map(|id|get(db,&id)).collect::<Result<Vec<_>,_>>()?}))
        }
        "project.alias" | "project.root" => {
            let id = required_text(args, "project_id")?;
            get(db, id)?;
            let path = directory(required_text(args, "path")?)?;
            if let Some(existing) = owner(db, &path)? {
                if existing != id {
                    return Err(Fault::new(
                        "PROJECT_PATH_CONFLICT",
                        6,
                        "Path is already associated with another project",
                    ));
                }
            } else {
                db.execute(
                    "INSERT INTO project_paths(path,project_id) VALUES(?1,?2)",
                    params![path, id],
                )?;
            }
            if operation == "project.root" {
                db.execute(
                    "UPDATE projects SET canonical_root=?1 WHERE id=?2",
                    params![path, id],
                )?;
            }
            get(db, id)
        }
        _ => Err(Fault::new(
            "UNKNOWN_OPERATION",
            2,
            "Project operation is not supported",
        )),
    }
}
