use std::path::Path;

pub trait PathExt {
    fn rootless(&self) -> &Path;
}

impl PathExt for Path {
    fn rootless(&self) -> &Path {
        self.strip_prefix("/").unwrap_or(self)
    }
}
