//! Where the worker's own secrets come from.
//!
//! A secret can be given as the variable itself (`NAME=...`) or as the path
//! of a file that holds it (`NAME_FILE=/path`). The file form keeps the value
//! out of the process environment, where it shows up in `/proc/<pid>/environ`
//! and in `docker inspect`, and matches how Docker and systemd mount secrets.

use std::fs;

/// Returns the secret called `name`, or `None` if neither `name` nor
/// `name_FILE` is set. A secret file that others on the machine can read is
/// refused rather than used.
pub fn read_secret(name: &str) -> Result<Option<String>, String> {
    let file_variable = format!("{name}_FILE");
    if let Ok(path) = std::env::var(&file_variable) {
        return read_secret_file(&file_variable, &path).map(Some);
    }
    Ok(std::env::var(name).ok())
}

fn read_secret_file(variable: &str, path: &str) -> Result<String, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|e| format!("{variable}: cannot read {path}: {e}"))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "{variable}: {path} is readable by other users (mode {:o}); run chmod 600 on it",
                mode & 0o777
            ));
        }
    }
    let content = fs::read_to_string(path).map_err(|e| format!("{variable}: cannot read {path}: {e}"))?;
    let secret = content.trim().to_string();
    if secret.is_empty() {
        return Err(format!("{variable}: {path} is empty"));
    }
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // Each test uses its own variable name, so they can run in parallel.
    fn write(path: &std::path::Path, content: &str, mode: u32) {
        let mut file = fs::File::create(path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
        }
    }

    #[test]
    fn reads_the_plain_variable_when_there_is_no_file() {
        std::env::set_var("LZK_TEST_PLAIN", "abc");
        assert_eq!(read_secret("LZK_TEST_PLAIN").unwrap().as_deref(), Some("abc"));
        assert_eq!(read_secret("LZK_TEST_NOT_SET").unwrap(), None);
    }

    #[test]
    fn reads_the_file_when_one_is_named_and_trims_the_newline() {
        let dir = tempfile_dir("file");
        let path = dir.join("secret");
        write(&path, "from-file\n", 0o600);
        std::env::set_var("LZK_TEST_FILE_FILE", path.to_str().unwrap());
        std::env::set_var("LZK_TEST_FILE", "from-env");
        assert_eq!(read_secret("LZK_TEST_FILE").unwrap().as_deref(), Some("from-file"), "the file wins over the variable");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_file_other_users_can_read() {
        let dir = tempfile_dir("open");
        let path = dir.join("secret");
        write(&path, "value", 0o644);
        std::env::set_var("LZK_TEST_OPEN_FILE", path.to_str().unwrap());
        let error = read_secret("LZK_TEST_OPEN").unwrap_err();
        assert!(error.contains("readable by other users"), "{error}");
        assert!(!error.contains("value"), "the secret must not appear in the error");
    }

    #[test]
    fn refuses_a_missing_or_empty_file() {
        std::env::set_var("LZK_TEST_MISSING_FILE", "/nonexistent/lzk-secret");
        assert!(read_secret("LZK_TEST_MISSING").is_err());
        let dir = tempfile_dir("empty");
        let path = dir.join("secret");
        write(&path, "  \n", 0o600);
        std::env::set_var("LZK_TEST_EMPTY_FILE", path.to_str().unwrap());
        assert!(read_secret("LZK_TEST_EMPTY").unwrap_err().contains("empty"));
    }

    fn tempfile_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lzk-secrets-{}-{tag}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
