//! What a child process inherits of this process's environment: everything but RichView's own
//! variables.
//!
//! `RICHVIEW_TOKEN`, a server's sign-in client secrets, its SMTP password, its administrator
//! password and its operator token all live in `RICHVIEW_` variables. The browser that prints a
//! PDF runs the report author's page, and MetricFlow runs the author's project, so neither may
//! read them. No child reads any `RICHVIEW_` variable, so every one is withheld, and a secret
//! added later needs no entry here.

use std::{
    ffi::{OsStr, OsString},
    process::Command,
};

const OWN: &[u8] = b"RICHVIEW_";

/// Whether `name` is one of RichView's own variables. Windows names its variables without
/// regard to case, so `richview_token` there is `RICHVIEW_TOKEN`.
pub(crate) fn is_own(name: &OsStr) -> bool {
    name.as_encoded_bytes()
        .get(..OWN.len())
        .is_some_and(|head| {
            if cfg!(windows) {
                head.eq_ignore_ascii_case(OWN)
            } else {
                head == OWN
            }
        })
}

/// `vars` without RichView's own variables, in their order.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn inherited(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    vars.into_iter().filter(|(name, _)| !is_own(name)).collect()
}

/// Withhold RichView's own variables, as this process has them now, from `command`'s child.
/// Variables the caller sets on the command afterwards still reach it.
pub(crate) fn withhold(command: &mut Command) {
    withhold_of(command, std::env::vars_os());
}

fn withhold_of(command: &mut Command, vars: impl IntoIterator<Item = (OsString, OsString)>) {
    for (name, _) in vars {
        if is_own(&name) {
            command.env_remove(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(names: &[&str]) -> Vec<(OsString, OsString)> {
        names
            .iter()
            .map(|name| (OsString::from(name), OsString::from("value")))
            .collect()
    }

    #[test]
    fn every_richview_variable_is_withheld_and_every_other_one_inherited() {
        let kept = inherited(vars(&[
            "PATH",
            "RICHVIEW_TOKEN",
            "HOME",
            "RICHVIEW_SMTP_PASSWORD",
            "RICHVIEW_GITHUB_CLIENT_SECRET",
            "RICHVIEW_MICROSOFT_CLIENT_SECRET",
            "RICHVIEW_GOOGLE_CLIENT_SECRET",
            "RICHVIEW_OPERATOR_TOKEN",
            "RICHVIEW_ADMIN_PASSWORD",
            "RICHVIEW_SERVER",
            "RICHVIEW_",
            "RICHVIEW",
            "MY_RICHVIEW_TOKEN",
            "DISPLAY",
        ]));
        let names: Vec<&str> = kept.iter().filter_map(|(name, _)| name.to_str()).collect();
        assert_eq!(
            names,
            ["PATH", "HOME", "RICHVIEW", "MY_RICHVIEW_TOKEN", "DISPLAY"]
        );
    }

    #[test]
    fn a_name_in_another_case_is_the_same_variable_only_on_windows() {
        assert_eq!(is_own(OsStr::new("richview_token")), cfg!(windows));
        assert_eq!(is_own(OsStr::new("RichView_Token")), cfg!(windows));
    }

    #[test]
    fn a_command_withholds_them_and_keeps_what_the_caller_sets_afterwards() {
        let mut command = Command::new("child");
        withhold_of(
            &mut command,
            vars(&["PATH", "RICHVIEW_TOKEN", "RICHVIEW_SERVER"]),
        );
        command.env("RICHVIEW_SERVER", "set by the caller");
        let changed: Vec<(&str, Option<&str>)> = command
            .get_envs()
            .map(|(name, value)| (name.to_str().unwrap(), value.and_then(OsStr::to_str)))
            .collect();
        assert_eq!(
            changed,
            [
                ("RICHVIEW_SERVER", Some("set by the caller")),
                ("RICHVIEW_TOKEN", None)
            ]
        );
    }

    /// The child itself, not only the command's list: `env` prints what it received.
    #[cfg(unix)]
    #[test]
    fn a_started_child_receives_none_of_them() {
        let mut command = Command::new("env");
        command
            .env("RICHVIEW_TOKEN", "sentinel-token")
            .env("RICHVIEW_OPERATOR_TOKEN", "sentinel-operator")
            .env("RICHVIEW_KEPT_ELSEWHERE", "sentinel-other");
        let set: Vec<(OsString, OsString)> = command
            .get_envs()
            .filter_map(|(name, value)| Some((name.to_owned(), value?.to_owned())))
            .collect();
        let with = String::from_utf8(command.output().unwrap().stdout).unwrap();
        assert!(with.contains("RICHVIEW_TOKEN=sentinel-token"), "{with}");
        let mut command = Command::new("env");
        command.envs(set.iter().map(|(name, value)| (name, value)));
        withhold_of(&mut command, set.iter().cloned());
        let without = String::from_utf8(command.output().unwrap().stdout).unwrap();
        assert!(!without.contains("sentinel"), "{without}");
        assert!(without.contains("PATH="), "{without}");
    }
}
