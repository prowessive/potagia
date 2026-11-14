use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Loads typed configuration values from a YAML file.
#[derive(Debug)]
pub struct YamlConfigLoader {
    path: PathBuf,
}

impl YamlConfigLoader {
    /// Creates a loader for the configuration file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Reads and deserializes the configuration file into `T`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigLoadError::Read`] if the file cannot be read, or
    /// [`ConfigLoadError::Parse`] if its contents are not valid YAML for `T`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use potagia::config::YamlConfigLoader;
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Settings {
    ///     port: u16,
    /// }
    ///
    /// let settings: Settings = YamlConfigLoader::new("settings.yaml").load()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load<T>(&self) -> Result<T, ConfigLoadError>
    where
        T: DeserializeOwned,
    {
        let contents = fs::read_to_string(&self.path).map_err(|source| ConfigLoadError::Read {
            path: self.path.clone(),
            source,
        })?;

        serde_yaml::from_str(&contents).map_err(|source| ConfigLoadError::Parse {
            path: self.path.clone(),
            source,
        })
    }
}

/// An error encountered while loading a YAML configuration file.
#[derive(Debug)]
pub enum ConfigLoadError {
    /// The configuration file could not be read.
    Read {
        /// Path of the configuration file.
        path: PathBuf,
        /// Underlying filesystem error.
        source: io::Error,
    },
    /// The configuration file could not be deserialized into the requested type.
    Parse {
        /// Path of the configuration file.
        path: PathBuf,
        /// Underlying YAML deserialization error.
        source: serde_yaml::Error,
    },
}

impl ConfigLoadError {
    /// Returns the path of the configuration file that caused the error.
    pub fn path(&self) -> &Path {
        match self {
            Self::Read { path, .. } | Self::Parse { path, .. } => path,
        }
    }
}

impl Display for ConfigLoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(
                formatter,
                "could not read YAML configuration from '{}': {source}",
                path.display()
            ),
            Self::Parse { path, source } => write!(
                formatter,
                "could not parse YAML configuration from '{}': {source}",
                path.display()
            ),
        }
    }
}

impl Error for ConfigLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[derive(Debug, Deserialize, PartialEq)]
    struct AppConfig {
        name: String,
        port: u16,
        features: Vec<String>,
    }

    #[test]
    fn loads_valid_yaml_into_the_requested_type() {
        let file = yaml_file("name: Configuración\nport: 8080\nfeatures:\n  - http\n  - metrics\n");
        let loader = YamlConfigLoader::new(file.path());

        let config: AppConfig = loader.load().expect("valid YAML should load");

        assert_eq!(
            config,
            AppConfig {
                name: "Configuración".to_owned(),
                port: 8080,
                features: vec!["http".to_owned(), "metrics".to_owned()],
            }
        );
    }

    #[test]
    fn returns_parse_error_for_yaml_that_does_not_match_the_requested_type() {
        let file = yaml_file("name: example\nport: not-a-number\nfeatures: []\n");
        let loader = YamlConfigLoader::new(file.path());

        let error = loader
            .load::<AppConfig>()
            .expect_err("an invalid port should fail deserialization");

        assert!(matches!(error, ConfigLoadError::Parse { .. }));
        assert_eq!(error.path(), file.path());
    }

    #[test]
    fn returns_read_error_when_the_file_does_not_exist() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let missing_path = directory.path().join("missing.yaml");
        let loader = YamlConfigLoader::new(&missing_path);

        let error = loader
            .load::<AppConfig>()
            .expect_err("a missing file should fail to load");

        match error {
            ConfigLoadError::Read { path, source } => {
                assert_eq!(path, missing_path);
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
            }
            ConfigLoadError::Parse { .. } => panic!("expected a read error"),
        }
    }

    fn yaml_file(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("temporary YAML file should be created");
        file.write_all(contents.as_bytes())
            .expect("YAML contents should be written");
        file
    }
}
