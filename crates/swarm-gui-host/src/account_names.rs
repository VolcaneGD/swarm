//! Non-authoritative, data-root-local name suggestions, protected by the GUI DPAPI store.
use swarm_platform::CredentialStore;

const KEY: &str = "account-name-history";
const LIMIT: usize = 20;

fn valid(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 4096 && !name.chars().any(char::is_control)
}

pub(super) fn read(store: &CredentialStore) -> Vec<String> {
    store
        .get(KEY)
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|name| valid(name))
        .take(LIMIT)
        .collect()
}

pub(super) fn remember(store: &CredentialStore, name: &str) {
    if !valid(name) {
        return;
    }
    let mut names = read(store);
    if names.first().is_some_and(|first| first == name) {
        return;
    }
    names.retain(|previous| previous != name);
    names.insert(0, name.to_owned());
    names.truncate(LIMIT);
    // Suggestions must never turn a committed login/change into an authentication failure.
    if let Ok(bytes) = serde_json::to_vec(&names) {
        let _ = store.put(KEY, &bytes);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn names_are_bounded_private_to_the_root_and_survive_reopen() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let path = root.path().join("gui-state");
        let store = CredentialStore::open(&path).unwrap();
        for index in 0..25 {
            remember(&store, &format!("Member{index}"));
        }
        remember(&store, "Member10");
        remember(&store, "\ninvalid");
        let names = read(&CredentialStore::open(&path).unwrap());
        assert_eq!(names.len(), LIMIT);
        assert_eq!(names[0], "Member10");
        assert_eq!(
            names
                .iter()
                .filter(|name| name.as_str() == "Member10")
                .count(),
            1
        );
        assert!(read(&CredentialStore::open(&other.path().join("gui-state")).unwrap()).is_empty());
        store.put(KEY, b"invalid json").unwrap();
        assert!(read(&store).is_empty());
    }
}
