use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    version: u32,
    account_id: String,
    alias: String,
    directory: PathBuf,
    key: String,
    input_sha256: String,
    targets: Value,
    intent: Option<String>,
}
fn key(account: &str) -> String {
    format!("restore-selection-{:x}", Sha256::digest(account.as_bytes()))
}
fn hash_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl GuiHost {
    /// Resolve the saved destination, never an unsaved renderer draft or last picker folder.
    pub fn restore_picker_directory(&self, account: &str) -> Result<PathBuf, Fault> {
        let (client, alias) = self.restore_client(account)?;
        let settings = client.backup(&alias, account, "settings.get", json!({}))?;
        match settings.get("destination") {
            Some(Value::String(destination)) => Ok(PathBuf::from(destination)),
            Some(Value::Null) => Ok(self
                .root
                .canonicalize()
                .map_err(|_| storage())?
                .join("backups")),
            _ => Err(storage()),
        }
    }

    pub fn check_restore_access(&self, account: &str) -> Result<(), Fault> {
        self.restore_client(account).map(|_| ())
    }
    fn restore_client(&self, account: &str) -> Result<(GuiClient, String), Fault> {
        if account.is_empty() || account.len() > 128 {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        client.backup(&selected.alias, account, "settings.get", json!({}))?;
        Ok((client, selected.alias))
    }
    fn restore_selection(&self, account: &str) -> Result<Option<Selection>, Fault> {
        self.store
            .get(&key(account))
            .map_err(|_| storage())?
            .map(|bytes| {
                let s: Selection = serde_json::from_slice(&bytes).map_err(|_| storage())?;
                if s.version != 1
                    || s.account_id != account
                    || !valid_alias(&s.alias)
                    || !s.directory.is_absolute()
                    || s.key.is_empty()
                    || s.key.len() > 128
                    || !hash_valid(&s.input_sha256)
                    || s.targets.as_array().is_none_or(|a| a.is_empty())
                    || s.intent
                        .as_ref()
                        .is_some_and(|i| !swarm_core::valid_backup_intent(i))
                {
                    return Err(storage());
                }
                Ok(s)
            })
            .transpose()
    }
    fn save_restore_selection(&self, s: &Selection) -> Result<(), Fault> {
        self.store
            .put(
                &key(&s.account_id),
                &serde_json::to_vec(s).map_err(|_| storage())?,
            )
            .map_err(|_| storage())
    }
    fn restore_preview(
        client: &GuiClient,
        alias: &str,
        account: &str,
        s: &Selection,
    ) -> Result<Value, Fault> {
        let preview = client.backup(
            alias,
            account,
            "restore.preview",
            json!({"directory":s.directory,"key":s.key}),
        )?;
        if preview["input_sha256"] != s.input_sha256 || preview["targets"] != s.targets {
            return Err(Fault::new(
                "RESTORE_CHANGED",
                6,
                "Selected backup changed; select and review it again",
            ));
        }
        Ok(preview)
    }
    pub fn select_restore_backup(&mut self, account: &str, path: &Path) -> Result<Value, Fault> {
        let (client, alias) = self.restore_client(account)?;
        if self
            .restore_selection(account)?
            .as_ref()
            .is_some_and(|s| s.intent.is_some())
        {
            return Err(conflict());
        }
        if !path.is_absolute() || path.extension().and_then(|s| s.to_str()) != Some("backup") {
            return Err(invalid());
        }
        let directory = path.parent().ok_or_else(invalid)?.to_path_buf();
        let file_key = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(invalid)?;
        let preview = client.backup(
            &alias,
            account,
            "restore.preview",
            json!({"directory":directory,"key":file_key}),
        )?;
        let s = Selection {
            version: 1,
            account_id: account.into(),
            alias,
            directory,
            key: file_key.into(),
            input_sha256: preview["input_sha256"]
                .as_str()
                .filter(|h| hash_valid(h))
                .ok_or_else(storage)?
                .into(),
            targets: preview["targets"].clone(),
            intent: None,
        };
        self.save_restore_selection(&s)?;
        Ok(
            json!({"state":"selected","path":path,"preview":preview,"preparation":null,"error_code":null}),
        )
    }
    pub fn restore_status(&self, account: &str) -> Result<Value, Fault> {
        let (client, alias) = self.restore_client(account)?;
        let Some(s) = self.restore_selection(account)? else {
            return Ok(Value::Null);
        };
        let path = s.directory.join(format!("{}.backup", s.key));
        if let Some(intent) = &s.intent {
            let result = client.backup(&alias, account, "restore.status", json!({"intent":intent}));
            if let Ok(p) = result
                && (p["state"] == "applied" || p["state"] == "rolled_back")
            {
                return Ok(
                    json!({"state":p["state"],"path":path,"preview":null,"preparation":p,"error_code":null,"can_retry":false}),
                );
            }
        }
        let preview = match Self::restore_preview(&client, &alias, account, &s) {
            Ok(p) => p,
            Err(e) => {
                return Ok(
                    json!({"state":"unavailable","path":path,"preview":null,"preparation":null,"error_code":e.code,"can_retry":s.intent.is_some()}),
                );
            }
        };
        let can_retry = s.intent.is_some();
        let preparation = match s.intent {
            None => Value::Null,
            Some(intent) => {
                match client.backup(&alias, account, "restore.status", json!({"intent":intent})) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(
                            json!({"state":"unavailable","path":path,"preview":preview,"preparation":null,"error_code":e.code,"can_retry":true}),
                        );
                    }
                }
            }
        };
        let state = if preparation.is_null() {
            "selected"
        } else if preparation["state"] == "prepared" {
            "prepared"
        } else {
            "preparing"
        };
        Ok(
            json!({"state":state,"path":path,"preview":preview,"preparation":preparation,"error_code":null,"can_retry":can_retry}),
        )
    }
    pub fn prepare_restore(&mut self, account: &str, confirmed: bool) -> Result<Value, Fault> {
        let (client, alias) = self.restore_client(account)?;
        if !confirmed {
            return Err(Fault::new(
                "RESTORE_CONFIRMATION",
                6,
                "Review the selected backup and destinations first",
            ));
        }
        let mut s = self.restore_selection(account)?.ok_or_else(conflict)?;
        Self::restore_preview(&client, &alias, account, &s)?;
        if s.intent.is_none() {
            s.intent = Some(uuid::Uuid::new_v4().to_string());
            self.save_restore_selection(&s)?;
        }
        client.backup(&alias,account,"restore.prepare",json!({"directory":s.directory,"key":s.key,"input_sha256":s.input_sha256,"intent":s.intent,"targets":s.targets}))?;
        self.restore_status(account)
    }
    /// Only a stored selection may be applied; renderer never supplies paths.
    pub fn apply_restore(&mut self, account: &str, confirmed: bool) -> Result<Value, Fault> {
        let (client, alias) = self.restore_client(account)?;
        if !confirmed {
            return Err(Fault::new(
                "RESTORE_CONFIRMATION",
                6,
                "Confirm replacement and reauthentication first",
            ));
        }
        let s = self.restore_selection(account)?.ok_or_else(conflict)?;
        let intent = s.intent.as_ref().ok_or_else(conflict)?;
        client.backup(
            &alias,
            account,
            "restore.apply",
            json!({"intent":intent,"input_sha256":s.input_sha256,"confirmed":true}),
        )
    }
    /// Clears the host selection only; runtime receipts and all archives remain.
    pub fn clear_restore_selection(&mut self, account: &str) -> Result<Value, Fault> {
        self.restore_client(account)?;
        self.store.remove(&key(account)).map_err(|_| storage())?;
        Ok(Value::Null)
    }
}
