use anyhow::Result;
use git2::{Repository, Signature, Time};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_path(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

struct TempHome {
    path: PathBuf,
}

struct TestRepo {
    path: PathBuf,
    repo: Repository,
}

impl TempHome {
    fn new() -> Result<Self> {
        let path = unique_path("gitlogue-cli-home");
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn config_path(&self) -> PathBuf {
        self.path.join(".config/gitlogue/config.toml")
    }
}

impl TestRepo {
    fn new() -> Result<Self> {
        let path = unique_path("gitlogue-cli-repo");
        fs::create_dir_all(&path)?;
        let repo = Repository::init(&path)?;
        Ok(Self { path, repo })
    }

    fn write_file(&self, relative_path: &str, content: &str) -> Result<()> {
        let file_path = self.path.join(relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file_path, content)?;
        Ok(())
    }

    fn stage_file(&self, relative_path: &str) -> Result<()> {
        let mut index = self.repo.index()?;
        index.add_path(Path::new(relative_path))?;
        index.write()?;
        Ok(())
    }

    fn commit_file(
        &self,
        relative_path: &str,
        content: &str,
        message: &str,
        timestamp: i64,
    ) -> Result<String> {
        self.write_file(relative_path, content)?;
        self.stage_file(relative_path)?;

        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let signature = Signature::new("Test User", "test@example.com", &Time::new(timestamp, 0))?;
        let parent = self
            .repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());
        let oid = match parent.as_ref() {
            Some(parent_commit) => self.repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[parent_commit],
            )?,
            None => self
                .repo
                .commit(Some("HEAD"), &signature, &signature, message, &tree, &[])?,
        };

        Ok(oid.to_string())
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn gitlogue_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gitlogue"))
}

fn command_with_home(home: &TempHome) -> Command {
    let mut command = gitlogue_command();
    command
        .env("HOME", &home.path)
        .env("USERPROFILE", &home.path)
        .env_remove("HOMEDRIVE")
        .env_remove("HOMEPATH");
    command
}

fn run_command(command: &mut Command) -> Result<Output> {
    Ok(command.output()?)
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn repo_path(repo: &TestRepo) -> &Path {
    repo.path.as_path()
}

#[test]
fn license_flag_prints_third_party_licenses() -> Result<()> {
    let output = run_command(gitlogue_command().arg("--license"))?;
    assert!(output.status.success());
    let stdout = stdout(&output);

    assert!(stdout.starts_with(include_str!("../LICENSE-THIRD-PARTY")));

    Ok(())
}

#[test]
fn theme_subcommands_list_and_set_default_theme() -> Result<()> {
    let home = TempHome::new()?;

    let list_output = run_command(command_with_home(&home).args(["theme", "list"]))?;
    assert!(list_output.status.success());
    let list_stdout = stdout(&list_output);
    assert!(list_stdout.contains("Available themes:"));
    assert!(list_stdout.contains("  - nord"));

    let set_output = run_command(command_with_home(&home).args(["theme", "set", "nord"]))?;
    assert!(set_output.status.success());
    let set_stdout = stdout(&set_output);
    let config = fs::read_to_string(home.config_path())?;

    assert!(set_stdout.contains("Theme set to 'nord'"));
    assert!(config.contains("theme = \"nord\""));

    Ok(())
}

#[test]
fn diff_subcommand_reports_no_changes_for_clean_repo() -> Result<()> {
    let repo = TestRepo::new()?;

    let output = run_command(gitlogue_command().args([
        "--path",
        repo_path(&repo).to_str().unwrap(),
        "diff",
    ]))?;
    assert!(output.status.success());

    assert_eq!(stdout(&output), "No changes to display\n");

    Ok(())
}

#[test]
fn diff_subcommand_with_staged_changes_fails_only_after_ui_startup_without_tty() -> Result<()> {
    let repo = TestRepo::new()?;
    repo.commit_file("src/lib.rs", "fn clean() {}\n", "initial", 1)?;
    repo.write_file("src/lib.rs", "fn staged() {}\n")?;
    repo.stage_file("src/lib.rs")?;

    let output = run_command(gitlogue_command().args([
        "--path",
        repo_path(&repo).to_str().unwrap(),
        "diff",
    ]))?;

    assert!(!output.status.success());
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("No such device or address"));

    Ok(())
}

#[test]
fn default_playback_fails_only_after_ui_startup_without_tty() -> Result<()> {
    let repo = TestRepo::new()?;
    repo.commit_file("src/main.rs", "fn main() {}\n", "initial", 1)?;

    let output =
        run_command(gitlogue_command().args(["--path", repo_path(&repo).to_str().unwrap()]))?;

    assert!(!output.status.success());
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("No such device or address"));

    Ok(())
}
