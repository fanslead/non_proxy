use std::{ffi::OsString, path::PathBuf};

use crate::AdminError;

#[derive(Debug, Eq, PartialEq)]
pub enum Command {
    Keygen {
        output: PathBuf,
    },
    Inspect {
        input: PathBuf,
    },
    Verify {
        endpoint: String,
        public_keys: Vec<String>,
    },
}

pub fn parse(values: impl IntoIterator<Item = OsString>) -> Result<Command, AdminError> {
    let values = values.into_iter().collect::<Vec<_>>();
    match values.as_slice() {
        [command, output_flag, output] if command == "keygen" && output_flag == "--output" => {
            Ok(Command::Keygen {
                output: PathBuf::from(output),
            })
        }
        [command, input_flag, input] if command == "inspect" && input_flag == "--input" => {
            Ok(Command::Inspect {
                input: PathBuf::from(input),
            })
        }
        [command, endpoint_flag, endpoint, keys_flag, public_keys]
            if command == "verify"
                && endpoint_flag == "--endpoint"
                && keys_flag == "--public-keys" =>
        {
            let endpoint = endpoint
                .to_str()
                .filter(|value| !value.is_empty())
                .ok_or(AdminError::Usage)?
                .to_owned();
            let public_keys = public_keys
                .to_str()
                .ok_or(AdminError::Usage)?
                .split(',')
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if public_keys.iter().any(String::is_empty) {
                return Err(AdminError::Usage);
            }
            Ok(Command::Verify {
                endpoint,
                public_keys,
            })
        }
        _ => Err(AdminError::Usage),
    }
}

pub const fn usage() -> &'static str {
    "用法：
  nonproxy-probe-admin keygen --output /绝对路径/signing-key.bin
  nonproxy-probe-admin inspect --input /绝对路径/signing-key.bin
  nonproxy-probe-admin verify --endpoint https://probe.example/v1/exit --public-keys <key[,key]>"
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{Command, parse};

    #[test]
    fn parses_each_bounded_command_and_rejects_ambiguous_input() {
        assert!(matches!(
            parse(values(["keygen", "--output", "/tmp/key"])),
            Ok(Command::Keygen { output }) if output == std::path::Path::new("/tmp/key")
        ));
        assert!(matches!(
            parse(values(["inspect", "--input", "/tmp/key"])),
            Ok(Command::Inspect { input }) if input == std::path::Path::new("/tmp/key")
        ));
        assert!(matches!(
            parse(values([
                "verify",
                "--endpoint",
                "https://probe.example/v1/exit",
                "--public-keys",
                "old,new",
            ])),
            Ok(Command::Verify { public_keys, .. }) if public_keys == ["old", "new"]
        ));
        assert!(parse(values(["keygen", "--output", "/tmp/key", "extra"])).is_err());
        assert!(
            parse(values([
                "verify",
                "--endpoint",
                "https://probe.example",
                "--public-keys",
                "old,",
            ]))
            .is_err()
        );
    }

    fn values<const LENGTH: usize>(values: [&str; LENGTH]) -> [OsString; LENGTH] {
        values.map(OsString::from)
    }
}
