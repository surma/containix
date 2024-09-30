use std::{
    ffi::{OsStr, OsString},
    fmt,
    str::FromStr,
};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct EnvVariable {
    pub key: OsString,
    pub value: OsString,
}

impl EnvVariable {
    pub fn new(key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        Self {
            key: key.as_ref().to_os_string(),
            value: value.as_ref().to_os_string(),
        }
    }

    pub fn to_os_string(&self) -> OsString {
        let mut s = OsString::new();
        s.push(&self.key);
        s.push("=");
        s.push(&self.value);
        s
    }
}

impl FromStr for EnvVariable {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let (key, value) = s
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("Invalid environment variable: {s}"))?;
        Ok(EnvVariable::new(key, value))
    }
}

impl fmt::Display for EnvVariable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}={}",
            self.key.to_string_lossy(),
            self.value.to_string_lossy()
        )
    }
}
