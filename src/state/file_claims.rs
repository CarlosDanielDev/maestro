use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Sentinel string that Claude must emit when it detects a file conflict.
pub const FILE_CONFLICT_SENTINEL: &str = "FILE_CONFLICT";

/// Manages exclusive file claims across sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileClaimManager {
    /// file_path -> session_id that owns it
    claims: HashMap<String, Uuid>,
    /// session_id -> set of claimed files (reverse index for fast release)
    session_files: HashMap<Uuid, HashSet<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClaimResult {
    /// File was successfully claimed by this session.
    Granted,
    /// File is already claimed by the same session (idempotent).
    AlreadyOwned,
    /// File is claimed by a different session.
    Conflict { owner: Uuid },
}

impl FileClaimManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt to claim a file for a session.
    pub fn claim(&mut self, file_path: &str, session_id: Uuid) -> ClaimResult {
        if let Some(&existing_owner) = self.claims.get(file_path) {
            if existing_owner == session_id {
                ClaimResult::AlreadyOwned
            } else {
                ClaimResult::Conflict { owner: existing_owner }
            }
        } else {
            self.claims.insert(file_path.to_string(), session_id);
            self.session_files
                .entry(session_id)
                .or_default()
                .insert(file_path.to_string());
            ClaimResult::Granted
        }
    }

    /// Release all claims for a session (on completion/kill).
    pub fn release_all(&mut self, session_id: Uuid) {
        if let Some(files) = self.session_files.remove(&session_id) {
            for file in files {
                self.claims.remove(&file);
            }
        }
    }

    /// Release a specific file claim.
    pub fn release(&mut self, file_path: &str, session_id: Uuid) {
        if self.claims.get(file_path) == Some(&session_id) {
            self.claims.remove(file_path);
            if let Some(files) = self.session_files.get_mut(&session_id) {
                files.remove(file_path);
            }
        }
    }

    /// Get all files claimed by a session.
    pub fn files_for_session(&self, session_id: Uuid) -> Vec<&str> {
        self.session_files
            .get(&session_id)
            .map(|files| files.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get the owning session for a file, if any.
    pub fn owner_of(&self, file_path: &str) -> Option<Uuid> {
        self.claims.get(file_path).copied()
    }

    /// Build the system prompt appendix for --append-system-prompt.
    /// Lists all files currently claimed by OTHER sessions so the target
    /// session knows to avoid them.
    pub fn build_system_prompt(&self, session_id: Uuid) -> Option<String> {
        let other_claims: Vec<&str> = self.claims
            .iter()
            .filter(|(_, owner)| **owner != session_id)
            .map(|(path, _)| path.as_str())
            .collect();

        if other_claims.is_empty() {
            return None;
        }

        let mut prompt = String::from(
            "MAESTRO COORDINATION: The following files are being modified by other agents. \
             DO NOT modify them. If you need to modify a claimed file, \
             output MAESTRO:FILE_CONFLICT:<path>\n\n"
        );

        for path in &other_claims {
            prompt.push_str(&format!("- CLAIMED: {}\n", path));
        }

        Some(prompt)
    }

    /// Total number of active claims.
    pub fn total_claims(&self) -> usize {
        self.claims.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn claim_new_file_returns_granted() {
        let mut mgr = FileClaimManager::new();
        let session = Uuid::new_v4();
        let result = mgr.claim("src/main.rs", session);
        assert!(matches!(result, ClaimResult::Granted));
    }

    #[test]
    fn claim_same_file_same_session_returns_already_owned() {
        let mut mgr = FileClaimManager::new();
        let session = Uuid::new_v4();
        mgr.claim("src/lib.rs", session);
        let result = mgr.claim("src/lib.rs", session);
        assert!(matches!(result, ClaimResult::AlreadyOwned));
    }

    #[test]
    fn claim_file_owned_by_other_session_returns_conflict() {
        let mut mgr = FileClaimManager::new();
        let owner = Uuid::new_v4();
        let intruder = Uuid::new_v4();
        mgr.claim("src/config.rs", owner);
        let result = mgr.claim("src/config.rs", intruder);
        match result {
            ClaimResult::Conflict { owner: reported } => assert_eq!(reported, owner),
            other => panic!("Expected Conflict, got {:?}", other),
        }
    }

    #[test]
    fn claim_multiple_files_for_same_session() {
        let mut mgr = FileClaimManager::new();
        let session = Uuid::new_v4();
        assert!(matches!(mgr.claim("src/a.rs", session), ClaimResult::Granted));
        assert!(matches!(mgr.claim("src/b.rs", session), ClaimResult::Granted));
        assert!(matches!(mgr.claim("src/c.rs", session), ClaimResult::Granted));
        assert_eq!(mgr.total_claims(), 3);
    }

    #[test]
    fn release_specific_file_allows_reclaim_by_other_session() {
        let mut mgr = FileClaimManager::new();
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();
        mgr.claim("src/foo.rs", session_a);
        mgr.release("src/foo.rs", session_a);
        let result = mgr.claim("src/foo.rs", session_b);
        assert!(matches!(result, ClaimResult::Granted));
    }

    #[test]
    fn release_file_not_owned_is_noop() {
        let mut mgr = FileClaimManager::new();
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();
        mgr.claim("src/bar.rs", session_a);
        mgr.release("src/bar.rs", session_b);
        assert_eq!(mgr.owner_of("src/bar.rs"), Some(session_a));
    }

    #[test]
    fn release_unclaimed_file_is_noop() {
        let mut mgr = FileClaimManager::new();
        let session = Uuid::new_v4();
        mgr.release("src/nonexistent.rs", session);
        assert_eq!(mgr.total_claims(), 0);
    }

    #[test]
    fn release_all_removes_all_claims_for_session() {
        let mut mgr = FileClaimManager::new();
        let session = Uuid::new_v4();
        mgr.claim("src/a.rs", session);
        mgr.claim("src/b.rs", session);
        mgr.claim("src/c.rs", session);
        mgr.release_all(session);
        assert_eq!(mgr.total_claims(), 0);
        assert!(mgr.files_for_session(session).is_empty());
    }

    #[test]
    fn release_all_does_not_affect_other_sessions() {
        let mut mgr = FileClaimManager::new();
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();
        mgr.claim("src/a.rs", session_a);
        mgr.claim("src/b.rs", session_b);
        mgr.release_all(session_a);
        assert_eq!(mgr.total_claims(), 1);
        assert_eq!(mgr.owner_of("src/b.rs"), Some(session_b));
    }

    #[test]
    fn release_all_on_session_with_no_claims_is_noop() {
        let mut mgr = FileClaimManager::new();
        mgr.release_all(Uuid::new_v4());
        assert_eq!(mgr.total_claims(), 0);
    }

    #[test]
    fn files_for_session_returns_correct_files() {
        let mut mgr = FileClaimManager::new();
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();
        mgr.claim("src/a.rs", session_a);
        mgr.claim("src/b.rs", session_a);
        mgr.claim("src/c.rs", session_b);
        let files = mgr.files_for_session(session_a);
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"src/a.rs"));
        assert!(files.contains(&"src/b.rs"));
    }

    #[test]
    fn files_for_session_returns_empty_for_unknown() {
        let mgr = FileClaimManager::new();
        assert!(mgr.files_for_session(Uuid::new_v4()).is_empty());
    }

    #[test]
    fn owner_of_returns_none_for_unclaimed() {
        let mgr = FileClaimManager::new();
        assert_eq!(mgr.owner_of("src/unclaimed.rs"), None);
    }

    #[test]
    fn owner_of_returns_correct_owner() {
        let mut mgr = FileClaimManager::new();
        let session = Uuid::new_v4();
        mgr.claim("src/owned.rs", session);
        assert_eq!(mgr.owner_of("src/owned.rs"), Some(session));
    }

    #[test]
    fn total_claims_starts_at_zero() {
        assert_eq!(FileClaimManager::new().total_claims(), 0);
    }

    #[test]
    fn total_claims_counts_across_sessions() {
        let mut mgr = FileClaimManager::new();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        mgr.claim("src/x.rs", s1);
        mgr.claim("src/y.rs", s1);
        mgr.claim("src/z.rs", s2);
        assert_eq!(mgr.total_claims(), 3);
    }

    #[test]
    fn build_system_prompt_none_when_no_other_claims() {
        let mgr = FileClaimManager::new();
        assert!(mgr.build_system_prompt(Uuid::new_v4()).is_none());
    }

    #[test]
    fn build_system_prompt_none_when_only_own_claims() {
        let mut mgr = FileClaimManager::new();
        let session = Uuid::new_v4();
        mgr.claim("src/mine.rs", session);
        assert!(mgr.build_system_prompt(session).is_none());
    }

    #[test]
    fn build_system_prompt_lists_other_sessions_files() {
        let mut mgr = FileClaimManager::new();
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();
        mgr.claim("src/a_file.rs", session_a);
        mgr.claim("src/b_file.rs", session_b);
        // Prompt for session_a should show session_b's files
        let prompt = mgr.build_system_prompt(session_a).unwrap();
        assert!(prompt.contains("src/b_file.rs"));
        assert!(!prompt.contains("src/a_file.rs"));
    }

    #[test]
    fn claim_then_release_then_reclaim() {
        let mut mgr = FileClaimManager::new();
        let session = Uuid::new_v4();
        mgr.claim("src/toggle.rs", session);
        mgr.release("src/toggle.rs", session);
        assert!(matches!(mgr.claim("src/toggle.rs", session), ClaimResult::Granted));
    }
}
