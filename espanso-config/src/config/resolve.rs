/*
 * This file is part of espanso.
 *
 * Copyright (C) 2019-2021 Federico Terzi
 *
 * espanso is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * espanso is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with espanso.  If not, see <https://www.gnu.org/licenses/>.
 */

use super::{parse::ParsedConfig, path::calculate_paths, util::os_matches, AppProperties, Config};
use crate::merge;
use anyhow::Result;
use regex::Regex;
use std::iter::FromIterator;
use std::{collections::HashSet, path::Path};
use thiserror::Error;

const STANDARD_INCLUDES: &[&str] = &["../match/**/*.yml"];
const STANDARD_EXCLUDES: &[&str] = &["../match/**/_*.yml"];

#[derive(Debug, Clone)]
pub(crate) struct ResolvedConfig {
  parsed: ParsedConfig,

  // Generated properties
  match_paths: Vec<String>,

  filter_title: Option<Regex>,
  filter_class: Option<Regex>,
  filter_exec: Option<Regex>,
}

impl Default for ResolvedConfig {
  fn default() -> Self {
    Self {
      parsed: Default::default(),
      match_paths: Vec::new(),
      filter_title: None,
      filter_class: None,
      filter_exec: None,
    }
  }
}

impl Config for ResolvedConfig {
  fn label(&self) -> &str {
    self.parsed.label.as_deref().unwrap_or("none")
  }

  fn match_paths(&self) -> &[String] {
    &self.match_paths
  }

  fn is_match(&self, app: &AppProperties) -> bool {
    if self.parsed.filter_os.is_none()
      && self.parsed.filter_title.is_none()
      && self.parsed.filter_exec.is_none()
      && self.parsed.filter_class.is_none()
    {
      return false;
    }

    let is_os_match = if let Some(filter_os) = self.parsed.filter_os.as_deref() {
      os_matches(filter_os)
    } else {
      true
    };

    let is_title_match = if let Some(title_regex) = self.filter_title.as_ref() {
      if let Some(title) = app.title {
        title_regex.is_match(title)
      } else {
        false
      }
    } else {
      true
    };

    let is_exec_match = if let Some(exec_regex) = self.filter_exec.as_ref() {
      if let Some(exec) = app.exec {
        exec_regex.is_match(exec)
      } else {
        false
      }
    } else {
      true
    };

    let is_class_match = if let Some(class_regex) = self.filter_class.as_ref() {
      if let Some(class) = app.class {
        class_regex.is_match(class)
      } else {
        false
      }
    } else {
      true
    };

    // All the filters that have been specified must be true to define a match
    is_os_match && is_exec_match && is_title_match && is_class_match
  }
}

impl ResolvedConfig {
  pub fn load(path: &Path, parent: Option<&Self>) -> Result<Self> {
    let mut config = ParsedConfig::load(path)?;

    // Merge with parent config if present
    if let Some(parent) = parent {
      Self::merge_parsed(&mut config, &parent.parsed);
    }

    // Extract the base directory
    let base_dir = path
      .parent()
      .ok_or_else(ResolveError::ParentResolveFailed)?;

    let match_paths = Self::generate_match_paths(&config, base_dir)
      .into_iter()
      .collect();

    let filter_title = if let Some(filter_title) = config.filter_title.as_deref() {
      Some(Regex::new(filter_title)?)
    } else {
      None
    };

    let filter_class = if let Some(filter_class) = config.filter_class.as_deref() {
      Some(Regex::new(filter_class)?)
    } else {
      None
    };

    let filter_exec = if let Some(filter_exec) = config.filter_exec.as_deref() {
      Some(Regex::new(filter_exec)?)
    } else {
      None
    };

    Ok(Self {
      parsed: config,
      match_paths,
      filter_title,
      filter_class,
      filter_exec,
    })
  }

  fn merge_parsed(child: &mut ParsedConfig, parent: &ParsedConfig) {
    // Override the None fields with the parent's value
    merge!(
      ParsedConfig,
      child,
      parent,
      // Fields
      label,
      includes,
      excludes,
      extra_includes,
      extra_excludes,
      use_standard_includes,
      filter_title,
      filter_class,
      filter_exec,
      filter_os
    );
  }

  fn aggregate_includes(config: &ParsedConfig) -> HashSet<String> {
    let mut includes = HashSet::new();

    if config.use_standard_includes.is_none() || config.use_standard_includes.unwrap() {
      STANDARD_INCLUDES.iter().for_each(|include| {
        includes.insert(include.to_string());
      })
    }

    if let Some(yaml_includes) = config.includes.as_ref() {
      yaml_includes.iter().for_each(|include| {
        includes.insert(include.to_string());
      })
    }

    if let Some(extra_includes) = config.extra_includes.as_ref() {
      extra_includes.iter().for_each(|include| {
        includes.insert(include.to_string());
      })
    }

    includes
  }

  fn aggregate_excludes(config: &ParsedConfig) -> HashSet<String> {
    let mut excludes = HashSet::new();

    if config.use_standard_includes.is_none() || config.use_standard_includes.unwrap() {
      STANDARD_EXCLUDES.iter().for_each(|exclude| {
        excludes.insert(exclude.to_string());
      })
    }

    if let Some(yaml_excludes) = config.excludes.as_ref() {
      yaml_excludes.iter().for_each(|exclude| {
        excludes.insert(exclude.to_string());
      })
    }

    if let Some(extra_excludes) = config.extra_excludes.as_ref() {
      extra_excludes.iter().for_each(|exclude| {
        excludes.insert(exclude.to_string());
      })
    }

    excludes
  }

  fn generate_match_paths(config: &ParsedConfig, base_dir: &Path) -> HashSet<String> {
    let includes = Self::aggregate_includes(config);
    let excludes = Self::aggregate_excludes(config);

    // Extract the paths
    let exclude_paths = calculate_paths(base_dir, excludes.iter());
    let include_paths = calculate_paths(base_dir, includes.iter());

    HashSet::from_iter(include_paths.difference(&exclude_paths).cloned())
  }
}

#[derive(Error, Debug)]
pub enum ResolveError {
  #[error("unable to resolve parent path")]
  ParentResolveFailed(),
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::util::tests::use_test_directory;
  use std::fs::create_dir_all;
  use std::iter::FromIterator;

  #[test]
  fn aggregate_includes_empty_config() {
    assert_eq!(
      ResolvedConfig::aggregate_includes(&ParsedConfig {
        ..Default::default()
      }),
      HashSet::from_iter(vec!["../match/**/*.yml".to_string(),].iter().cloned())
    );
  }

  #[test]
  fn aggregate_includes_no_standard() {
    assert_eq!(
      ResolvedConfig::aggregate_includes(&ParsedConfig {
        use_standard_includes: Some(false),
        ..Default::default()
      }),
      HashSet::new()
    );
  }

  #[test]
  fn aggregate_includes_custom_includes() {
    assert_eq!(
      ResolvedConfig::aggregate_includes(&ParsedConfig {
        includes: Some(vec!["custom/*.yml".to_string()]),
        ..Default::default()
      }),
      HashSet::from_iter(
        vec!["../match/**/*.yml".to_string(), "custom/*.yml".to_string()]
          .iter()
          .cloned()
      )
    );
  }

  #[test]
  fn aggregate_includes_extra_includes() {
    assert_eq!(
      ResolvedConfig::aggregate_includes(&ParsedConfig {
        extra_includes: Some(vec!["custom/*.yml".to_string()]),
        ..Default::default()
      }),
      HashSet::from_iter(
        vec!["../match/**/*.yml".to_string(), "custom/*.yml".to_string()]
          .iter()
          .cloned()
      )
    );
  }

  #[test]
  fn aggregate_includes_includes_and_extra_includes() {
    assert_eq!(
      ResolvedConfig::aggregate_includes(&ParsedConfig {
        includes: Some(vec!["sub/*.yml".to_string()]),
        extra_includes: Some(vec!["custom/*.yml".to_string()]),
        ..Default::default()
      }),
      HashSet::from_iter(
        vec![
          "../match/**/*.yml".to_string(),
          "custom/*.yml".to_string(),
          "sub/*.yml".to_string()
        ]
        .iter()
        .cloned()
      )
    );
  }

  #[test]
  fn aggregate_excludes_empty_config() {
    assert_eq!(
      ResolvedConfig::aggregate_excludes(&ParsedConfig {
        ..Default::default()
      }),
      HashSet::from_iter(vec!["../match/**/_*.yml".to_string(),].iter().cloned())
    );
  }

  #[test]
  fn aggregate_excludes_no_standard() {
    assert_eq!(
      ResolvedConfig::aggregate_excludes(&ParsedConfig {
        use_standard_includes: Some(false),
        ..Default::default()
      }),
      HashSet::new()
    );
  }

  #[test]
  fn aggregate_excludes_custom_excludes() {
    assert_eq!(
      ResolvedConfig::aggregate_excludes(&ParsedConfig {
        excludes: Some(vec!["custom/*.yml".to_string()]),
        ..Default::default()
      }),
      HashSet::from_iter(
        vec!["../match/**/_*.yml".to_string(), "custom/*.yml".to_string()]
          .iter()
          .cloned()
      )
    );
  }

  #[test]
  fn aggregate_excludes_extra_excludes() {
    assert_eq!(
      ResolvedConfig::aggregate_excludes(&ParsedConfig {
        extra_excludes: Some(vec!["custom/*.yml".to_string()]),
        ..Default::default()
      }),
      HashSet::from_iter(
        vec!["../match/**/_*.yml".to_string(), "custom/*.yml".to_string()]
          .iter()
          .cloned()
      )
    );
  }

  #[test]
  fn aggregate_excludes_excludes_and_extra_excludes() {
    assert_eq!(
      ResolvedConfig::aggregate_excludes(&ParsedConfig {
        excludes: Some(vec!["sub/*.yml".to_string()]),
        extra_excludes: Some(vec!["custom/*.yml".to_string()]),
        ..Default::default()
      }),
      HashSet::from_iter(
        vec![
          "../match/**/_*.yml".to_string(),
          "custom/*.yml".to_string(),
          "sub/*.yml".to_string()
        ]
        .iter()
        .cloned()
      )
    );
  }

  #[test]
  fn merge_parent_field_parent_fallback() {
    let parent = ParsedConfig {
      use_standard_includes: Some(false),
      ..Default::default()
    };
    let mut child = ParsedConfig {
      ..Default::default()
    };
    assert_eq!(child.use_standard_includes, None);

    ResolvedConfig::merge_parsed(&mut child, &parent);
    assert_eq!(child.use_standard_includes, Some(false));
  }

  #[test]
  fn merge_parent_field_child_overwrite_parent() {
    let parent = ParsedConfig {
      use_standard_includes: Some(true),
      ..Default::default()
    };
    let mut child = ParsedConfig {
      use_standard_includes: Some(false),
      ..Default::default()
    };
    assert_eq!(child.use_standard_includes, Some(false));

    ResolvedConfig::merge_parsed(&mut child, &parent);
    assert_eq!(child.use_standard_includes, Some(false));
  }

  #[test]
  fn match_paths_generated_correctly() {
    use_test_directory(|_, match_dir, config_dir| {
      let sub_dir = match_dir.join("sub");
      create_dir_all(&sub_dir).unwrap();

      let base_file = match_dir.join("base.yml");
      std::fs::write(&base_file, "test").unwrap();
      let another_file = match_dir.join("another.yml");
      std::fs::write(&another_file, "test").unwrap();
      let under_file = match_dir.join("_sub.yml");
      std::fs::write(&under_file, "test").unwrap();
      let sub_file = sub_dir.join("sub.yml");
      std::fs::write(&sub_file, "test").unwrap();

      let config_file = config_dir.join("default.yml");
      std::fs::write(&config_file, "").unwrap();

      let config = ResolvedConfig::load(&config_file, None).unwrap();

      let mut expected = vec![
        base_file.to_string_lossy().to_string(),
        another_file.to_string_lossy().to_string(),
        sub_file.to_string_lossy().to_string(),
      ];
      expected.sort();

      let mut result = config.match_paths().to_vec();
      result.sort();

      assert_eq!(result, expected.as_slice());
    });
  }

  #[test]
  fn match_paths_generated_correctly_with_child_config() {
    use_test_directory(|_, match_dir, config_dir| {
      let sub_dir = match_dir.join("sub");
      create_dir_all(&sub_dir).unwrap();

      let base_file = match_dir.join("base.yml");
      std::fs::write(&base_file, "test").unwrap();
      let another_file = match_dir.join("another.yml");
      std::fs::write(&another_file, "test").unwrap();
      let under_file = match_dir.join("_sub.yml");
      std::fs::write(&under_file, "test").unwrap();
      let sub_file = sub_dir.join("another.yml");
      std::fs::write(&sub_file, "test").unwrap();
      let sub_under_file = sub_dir.join("_sub.yml");
      std::fs::write(&sub_under_file, "test").unwrap();

      // Configs

      let parent_file = config_dir.join("parent.yml");
      std::fs::write(
        &parent_file,
        r#"
      excludes: ['../**/another.yml']
      "#,
      )
      .unwrap();

      let config_file = config_dir.join("default.yml");
      std::fs::write(
        &config_file,
        r#"
      use_standard_includes: false
      excludes: []
      includes: ["../match/sub/*.yml"]
      "#,
      )
      .unwrap();

      let parent = ResolvedConfig::load(&parent_file, None).unwrap();
      let child = ResolvedConfig::load(&config_file, Some(&parent)).unwrap();

      let mut expected = vec![
        sub_file.to_string_lossy().to_string(),
        sub_under_file.to_string_lossy().to_string(),
      ];
      expected.sort();

      let mut result = child.match_paths().to_vec();
      result.sort();
      assert_eq!(result, expected.as_slice());

      let expected = vec![base_file.to_string_lossy().to_string()];

      assert_eq!(parent.match_paths(), expected.as_slice());
    });
  }

  fn test_filter_is_match(config: &str, app: &AppProperties) -> bool {
    let mut result = false;
    let result_ref = &mut result;
    use_test_directory(move |_, _, config_dir| {
      let config_file = config_dir.join("default.yml");
      std::fs::write(&config_file, config).unwrap();

      let config = ResolvedConfig::load(&config_file, None).unwrap();

      *result_ref = config.is_match(app)
    });
    result
  }

  #[test]
  fn is_match_no_filters() {
    assert!(!test_filter_is_match(
      "",
      &AppProperties {
        title: Some("Google"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));
  }

  #[test]
  fn is_match_filter_title() {
    assert!(test_filter_is_match(
      "filter_title: Google",
      &AppProperties {
        title: Some("Google Mail"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));

    assert!(!test_filter_is_match(
      "filter_title: Google",
      &AppProperties {
        title: Some("Yahoo"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));

    assert!(!test_filter_is_match(
      "filter_title: Google",
      &AppProperties {
        title: None,
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));
  }

  #[test]
  fn is_match_filter_class() {
    assert!(test_filter_is_match(
      "filter_class: Chrome",
      &AppProperties {
        title: Some("Google Mail"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));

    assert!(!test_filter_is_match(
      "filter_class: Chrome",
      &AppProperties {
        title: Some("Yahoo"),
        class: Some("Another"),
        exec: Some("chrome.exe"),
      },
    ));

    assert!(!test_filter_is_match(
      "filter_class: Chrome",
      &AppProperties {
        title: Some("google"),
        class: None,
        exec: Some("chrome.exe"),
      },
    ));
  }

  #[test]
  fn is_match_filter_exec() {
    assert!(test_filter_is_match(
      "filter_exec: chrome.exe",
      &AppProperties {
        title: Some("Google Mail"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));

    assert!(!test_filter_is_match(
      "filter_exec: chrome.exe",
      &AppProperties {
        title: Some("Yahoo"),
        class: Some("Another"),
        exec: Some("zoom.exe"),
      },
    ));

    assert!(!test_filter_is_match(
      "filter_exec: chrome.exe",
      &AppProperties {
        title: Some("google"),
        class: Some("Chrome"),
        exec: None,
      },
    ));
  }

  #[test]
  fn is_match_filter_os() {
    let (current, another) = if cfg!(target_os = "windows") {
      ("windows", "macos")
    } else if cfg!(target_os = "macos") {
      ("macos", "windows")
    } else if cfg!(target_os = "linux") {
      ("linux", "macos")
    } else {
      ("invalid", "invalid")
    };

    assert!(test_filter_is_match(
      &format!("filter_os: {}", current),
      &AppProperties {
        title: Some("Google Mail"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));

    assert!(!test_filter_is_match(
      &format!("filter_os: {}", another),
      &AppProperties {
        title: Some("Google Mail"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));
  }

  #[test]
  fn is_match_multiple_filters() {
    assert!(test_filter_is_match(
      r#"
      filter_exec: chrome.exe
      filter_title: "Youtube"
      "#,
      &AppProperties {
        title: Some("Youtube - Broadcast Yourself"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));

    assert!(!test_filter_is_match(
      r#"
      filter_exec: chrome.exe
      filter_title: "Youtube"
      "#,
      &AppProperties {
        title: Some("Gmail"),
        class: Some("Chrome"),
        exec: Some("chrome.exe"),
      },
    ));
  }
}