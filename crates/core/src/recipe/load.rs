use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::error::{LoadErrors, RecipeError};
use super::model::{RawRecipe, Recipe};
use super::validate::validate;

/// Parses and validates one recipe. `path` is only used in error messages.
pub fn parse_recipe(text: &str, path: &Path) -> Result<Recipe, RecipeError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let de = serde_yaml_ng::Deserializer::from_str(text);
    let raw: RawRecipe = serde_path_to_error::deserialize(de).map_err(|e| {
        let message = e.inner().to_string();
        RecipeError::Invalid {
            path: path.to_path_buf(),
            field: field_path(&e.path().to_string(), &message),
            message,
        }
    })?;
    validate(raw, path)
}

/// Field path for an error. serde reports a missing field at its *parent*, so for
/// "missing field `why`" at the root this returns `why` instead of `.`.
fn field_path(parent: &str, message: &str) -> String {
    let parent = if parent == "." { "" } else { parent };
    let missing = message
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split('`').next());
    match (parent, missing) {
        ("", Some(name)) => name.to_owned(),
        ("", None) => "<root>".to_owned(),
        (parent, Some(name)) => format!("{parent}.{name}"),
        (parent, None) => parent.to_owned(),
    }
}

/// Loads every `*.yaml` / `*.yml` recipe below `dir` (recursively), sorted by id.
///
/// Fails if any file is invalid, if two recipes share an id, or if there are no recipes;
/// all problems are reported together.
pub fn load_dir(dir: &Path) -> Result<Vec<Recipe>, LoadErrors> {
    let mut errors = Vec::new();
    let mut files = Vec::new();
    collect_yaml_files(dir, &mut files, &mut errors);
    files.sort();

    let mut recipes = Vec::new();
    for file in files {
        match fs::read_to_string(&file) {
            Ok(text) => match parse_recipe(&text, &file) {
                Ok(recipe) => recipes.push(recipe),
                Err(e) => errors.push(e),
            },
            Err(source) => errors.push(RecipeError::Io { path: file, source }),
        }
    }
    finish(recipes, errors, dir)
}

/// Loads recipes compiled into the program, as `(relative path, YAML text)` pairs.
/// Same checks as [`load_dir`].
pub fn load_embedded(files: &[(&str, &str)]) -> Result<Vec<Recipe>, LoadErrors> {
    let mut recipes = Vec::new();
    let mut errors = Vec::new();
    for (path, text) in files {
        match parse_recipe(text, Path::new(path)) {
            Ok(recipe) => recipes.push(recipe),
            Err(e) => errors.push(e),
        }
    }
    finish(recipes, errors, Path::new("<built-in recipes>"))
}

fn finish(
    mut recipes: Vec<Recipe>,
    mut errors: Vec<RecipeError>,
    dir: &Path,
) -> Result<Vec<Recipe>, LoadErrors> {
    let mut first_seen: HashMap<&str, &Path> = HashMap::new();
    for recipe in &recipes {
        if let Some(first) = first_seen.insert(&recipe.id, &recipe.source_file) {
            errors.push(RecipeError::DuplicateId {
                id: recipe.id.clone(),
                first: first.to_path_buf(),
                second: recipe.source_file.clone(),
            });
        }
    }

    if recipes.is_empty() && errors.is_empty() {
        errors.push(RecipeError::NoRecipes {
            dir: dir.to_path_buf(),
        });
    }

    if errors.is_empty() {
        recipes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(recipes)
    } else {
        Err(LoadErrors(errors))
    }
}

fn collect_yaml_files(dir: &Path, files: &mut Vec<PathBuf>, errors: &mut Vec<RecipeError>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) => {
            errors.push(RecipeError::Io {
                path: dir.to_path_buf(),
                source,
            });
            return;
        }
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(source) => {
                errors.push(RecipeError::Io {
                    path: dir.to_path_buf(),
                    source,
                });
                continue;
            }
        };
        if path.is_dir() {
            collect_yaml_files(&path, files, errors);
        } else if is_yaml(&path) {
            files.push(path);
        }
    }
}

fn is_yaml(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"))
}

#[cfg(test)]
mod tests {
    use super::field_path;

    #[test]
    fn missing_fields_name_the_field() {
        assert_eq!(field_path(".", "missing field `why`"), "why");
        assert_eq!(
            field_path("platform", "missing field `min_build` at line 3"),
            "platform.min_build"
        );
        assert_eq!(field_path(".", "invalid type"), "<root>");
        assert_eq!(field_path("actions[0]", "unknown variant"), "actions[0]");
    }
}
