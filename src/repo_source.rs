use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Repo {
    pub path: PathBuf,
    pub base: PathBuf,
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl Repo {
    /// owner/repo
    pub fn short_key(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// host/owner/repo
    pub fn display_key(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.name)
    }

    /// Construct git URL from path components
    pub fn git_url(&self) -> String {
        format!("git@{}:{}/{}.git", self.host, self.owner, self.name)
    }
}

/// Scan base directories for git repositories.
/// Layout: base/host/owner/.../repo/.git (owner may contain nested groups)
pub fn scan(base_dirs: &[PathBuf]) -> Result<Vec<Repo>> {
    let mut repos = Vec::new();
    for base in base_dirs {
        if !base.exists() {
            continue;
        }
        scan_base(base, &mut repos)?;
    }
    repos.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(repos)
}

fn scan_base(base: &Path, repos: &mut Vec<Repo>) -> Result<()> {
    let base_path = base.to_path_buf();
    let Ok(hosts) = std::fs::read_dir(base) else {
        return Ok(());
    };
    for host_entry in hosts {
        let host_entry = host_entry?;
        if !host_entry.file_type()?.is_dir() {
            continue;
        }
        let host_name = host_entry.file_name().to_string_lossy().to_string();
        if host_name.starts_with('.') {
            continue;
        }
        scan_host_tree(&base_path, &host_name, &host_entry.path(), &host_entry.path(), repos)?;
    }
    Ok(())
}

/// Recursively walk host/owner/... until a directory containing `.git` is found.
fn scan_host_tree(
    base: &Path,
    host: &str,
    host_root: &Path,
    current: &Path,
    repos: &mut Vec<Repo>,
) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(current) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if dir_name.starts_with('.') {
            continue;
        }

        let dir_path = entry.path();
        if dir_path.join(".git").exists() {
            let rel = dir_path
                .strip_prefix(host_root)
                .unwrap_or(&dir_path)
                .to_string_lossy()
                .into_owned();
            let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() >= 2 {
                let owner = parts[0].to_string();
                let name = parts[1..].join("/");
                repos.push(Repo {
                    path: dir_path,
                    base: base.to_path_buf(),
                    host: host.to_string(),
                    owner,
                    name,
                });
            }
        } else {
            scan_host_tree(base, host, host_root, &dir_path, repos)?;
        }
    }
    Ok(())
}

/// Find repos matching a keyword (case-insensitive).
///
/// Returns all matches. Exact matches (ends with /keyword) are sorted first,
/// followed by partial matches (contains keyword).
pub fn find(repos: &[Repo], keyword: &str) -> Vec<Repo> {
    let kw_lower = keyword.to_lowercase();
    let keyword_suffix = format!("/{}", kw_lower.trim_start_matches('/'));

    let mut exact = Vec::new();
    let mut partial = Vec::new();

    for repo in repos {
        let key = repo.display_key().to_lowercase();
        if key.ends_with(&keyword_suffix) {
            exact.push(repo.clone());
        } else if key.contains(&kw_lower) {
            partial.push(repo.clone());
        }
    }

    exact.extend(partial);
    exact
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_repo(host: &str, owner: &str, name: &str) -> Repo {
        Repo {
            path: PathBuf::from(format!("/base/{host}/{owner}/{name}")),
            base: PathBuf::from("/base"),
            host: host.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn test_repo_display_key() {
        let repo = make_repo("github.com", "popomore", "projj");
        assert_eq!(repo.display_key(), "github.com/popomore/projj");
    }

    #[test]
    fn test_repo_short_key() {
        let repo = make_repo("github.com", "popomore", "projj");
        assert_eq!(repo.short_key(), "popomore/projj");
    }

    #[test]
    fn test_repo_git_url() {
        let repo = make_repo("github.com", "popomore", "projj");
        assert_eq!(repo.git_url(), "git@github.com:popomore/projj.git");
    }

    #[test]
    fn test_find_exact_match() {
        let repos = vec![
            make_repo("github.com", "popomore", "projj"),
            make_repo("github.com", "popomore", "tiny-projj"),
        ];
        let result = find(&repos, "projj");
        // Both match: "projj" exact first, "tiny-projj" partial second
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "projj");
        assert_eq!(result[1].name, "tiny-projj");
    }

    #[test]
    fn test_find_case_insensitive() {
        let repos = vec![make_repo("github.com", "popomore", "Projj")];
        let result = find(&repos, "projj");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_find_owner_repo() {
        let repos = vec![
            make_repo("github.com", "popomore", "projj"),
            make_repo("github.com", "other", "projj"),
        ];
        let result = find(&repos, "popomore/projj");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].owner, "popomore");
    }

    #[test]
    fn test_find_no_match() {
        let repos = vec![make_repo("github.com", "popomore", "projj")];
        let result = find(&repos, "nonexistent");
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_partial_match() {
        let repos = vec![
            make_repo("github.com", "popomore", "projj"),
            make_repo("github.com", "SeeleAI", "projj-tools"),
        ];
        let result = find(&repos, "projj");
        assert_eq!(result.len(), 2);
        // Exact match first
        assert_eq!(result[0].name, "projj");
        assert_eq!(result[1].name, "projj-tools");
    }

    #[test]
    fn test_scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn test_scan_with_repos() {
        let dir = tempfile::tempdir().unwrap();
        let repo_path = dir.path().join("github.com/popomore/projj/.git");
        std::fs::create_dir_all(&repo_path).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].host, "github.com");
        assert_eq!(repos[0].owner, "popomore");
        assert_eq!(repos[0].name, "projj");
    }

    #[test]
    fn test_scan_skips_hidden_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".hidden/owner/repo/.git")).unwrap();
        std::fs::create_dir_all(dir.path().join("github.com/.hidden/repo/.git")).unwrap();
        std::fs::create_dir_all(dir.path().join("github.com/owner/.hidden/.git")).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn test_scan_skips_non_git_dirs() {
        let dir = tempfile::tempdir().unwrap();
        // No .git directory
        std::fs::create_dir_all(dir.path().join("github.com/owner/repo")).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn test_scan_nonexistent_base() {
        let repos = scan(&[PathBuf::from("/nonexistent/path")]).unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn test_scan_multiple_bases() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir1.path().join("github.com/a/repo1/.git")).unwrap();
        std::fs::create_dir_all(dir2.path().join("gitlab.com/b/repo2/.git")).unwrap();
        let repos = scan(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]).unwrap();
        assert_eq!(repos.len(), 2);
    }

    #[test]
    fn test_scan_skips_files_at_host_level() {
        let dir = tempfile::tempdir().unwrap();
        // A file where a host dir should be
        std::fs::write(dir.path().join("not-a-dir"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("github.com/owner/repo/.git")).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(repos.len(), 1);
    }

    #[test]
    fn test_scan_skips_files_at_owner_level() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("github.com")).unwrap();
        std::fs::write(dir.path().join("github.com/not-a-dir"), "").unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn test_scan_skips_files_at_repo_level() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("github.com/owner")).unwrap();
        std::fs::write(dir.path().join("github.com/owner/not-a-dir"), "").unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn test_scan_sorted_output() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("github.com/z/repo/.git")).unwrap();
        std::fs::create_dir_all(dir.path().join("github.com/a/repo/.git")).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(repos.len(), 2);
        assert!(repos[0].path < repos[1].path);
    }

    #[test]
    fn test_scan_sets_base_field() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("github.com/owner/repo/.git")).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(repos[0].base, dir.path());
    }

    #[test]
    fn test_scan_nested_group_repo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("gitlab.com/team/subgroup/app/.git")).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].host, "gitlab.com");
        assert_eq!(repos[0].owner, "team");
        assert_eq!(repos[0].name, "subgroup/app");
        assert_eq!(repos[0].display_key(), "gitlab.com/team/subgroup/app");
    }

    #[test]
    fn test_scan_deeply_nested_repo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("gitlab.com/org/a/b/c/project/.git")).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].owner, "org");
        assert_eq!(repos[0].name, "a/b/c/project");
    }

    #[test]
    fn test_scan_mixed_depth_repos() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("github.com/popomore/projj/.git")).unwrap();
        std::fs::create_dir_all(dir.path().join("gitlab.com/team/sub/app/.git")).unwrap();
        let repos = scan(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(repos.len(), 2);
    }
}
