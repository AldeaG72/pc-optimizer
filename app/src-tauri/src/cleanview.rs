//! The cleanup recipes as the interface shows them: how much each would free, and running
//! the part that needs no administrator rights (the user's own temporary files, launcher
//! caches, Recycle Bin). System folders go through the helper (`clean` request).

use pco_core::cleanup::{needs_admin, CleanReport, CleanupBackend};
use pco_core::recipe::Recipe;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanSize {
    pub id: String,
    /// `None` when this side cannot see it (a system folder without administrator rights).
    pub bytes: Option<u64>,
    /// Some of it is in a system folder: cleaning shows the UAC prompt.
    pub needs_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub id: String,
    pub freed_bytes: u64,
    pub deleted: u64,
    pub skipped: u64,
    pub error: Option<String>,
}

fn cleanups(recipes: &[Recipe]) -> impl Iterator<Item = &Recipe> {
    recipes.iter().filter(|r| r.is_cleanup())
}

/// Measures every cleanup recipe. Changes nothing.
pub fn sizes(recipes: &[Recipe], user: &dyn CleanupBackend) -> Vec<CleanSize> {
    cleanups(recipes)
        .map(|r| {
            let mut total = Some(0u64);
            for target in r.cleanup_targets() {
                let size = user.measure(target).ok().flatten();
                total = total.zip(size).map(|(a, b)| a + b);
            }
            CleanSize {
                id: r.id.clone(),
                bytes: total,
                needs_admin: r.cleanup_targets().into_iter().any(needs_admin),
            }
        })
        .collect()
}

/// Cleans the user's own places of the given cleanup recipes; system folders are left
/// for the helper. Unknown or non-cleanup ids are refused.
pub fn clean_user(
    recipes: &[Recipe],
    ids: &[String],
    user: &mut dyn CleanupBackend,
) -> Result<Vec<CleanResult>, String> {
    let mut out = Vec::new();
    for id in ids {
        let Some(recipe) = cleanups(recipes).find(|r| &r.id == id) else {
            return Err(format!("not a cleanup: {id}"));
        };
        let mut report = CleanReport::default();
        let mut error = None;
        for target in recipe
            .cleanup_targets()
            .into_iter()
            .filter(|t| !needs_admin(*t))
        {
            match user.clean(target) {
                Ok(r) => report.add(r),
                Err(e) => error = Some(e.to_string()),
            }
        }
        out.push(CleanResult {
            id: id.clone(),
            freed_bytes: report.freed_bytes,
            deleted: report.deleted,
            skipped: report.skipped,
            error,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use pco_core::cleanup::MemoryCleanup;
    use pco_core::recipe::{self, CleanupTarget};

    fn recipes() -> Vec<Recipe> {
        recipe::load_embedded(pco_core::embedded::RECIPES).unwrap()
    }

    #[test]
    fn measures_and_cleans_only_the_users_own_places() {
        let all = recipes();
        let mut user = MemoryCleanup::default();
        user.sizes.insert(CleanupTarget::UserTemp, 1000);
        user.sizes.insert(CleanupTarget::RecycleBin, 50);

        let s = sizes(&all, &user);
        let temp = s.iter().find(|c| c.id == "cleanup.user-temp").unwrap();
        assert_eq!(temp.bytes, Some(1000));
        assert!(!temp.needs_admin);
        let system = s.iter().find(|c| c.id == "cleanup.windows-temp").unwrap();
        assert_eq!(system.bytes, None, "not visible without admin");
        assert!(system.needs_admin);

        let ids = vec![
            "cleanup.user-temp".to_owned(),
            "cleanup.windows-temp".to_owned(),
        ];
        let done = clean_user(&all, &ids, &mut user).unwrap();
        assert_eq!(done[0].freed_bytes, 1000);
        // The system folder is the helper's job: nothing happens here, and no error.
        assert_eq!(done[1].freed_bytes, 0);
        assert_eq!(done[1].error, None);

        assert!(clean_user(&all, &["gaming.game-mode-on".to_owned()], &mut user).is_err());
    }
}
