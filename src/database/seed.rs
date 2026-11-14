use super::{DatabaseError, query_error};
use tokio_postgres::Transaction;

#[derive(Debug)]
struct SeedFile {
    name: &'static str,
    content: &'static [u8],
    paths: &'static [&'static str],
}

const INITIAL_FILES: &[SeedFile] = &[
    SeedFile {
        name: "index.html",
        content: include_bytes!("../../magia/files/index.html"),
        paths: &["/", "/index.html"],
    },
    SeedFile {
        name: "styles.css",
        content: include_bytes!("../../magia/files/styles.css"),
        paths: &["/styles.css"],
    },
    SeedFile {
        name: "favicon.svg",
        content: include_bytes!("../../magia/files/assets/favicon.svg"),
        paths: &["/assets/favicon.svg"],
    },
];

pub(super) async fn seed_initial_content(
    transaction: &Transaction<'_>,
) -> Result<(), DatabaseError> {
    for file in INITIAL_FILES {
        let row = transaction
            .query_one(
                "INSERT INTO public.files (name, content) VALUES ($1, $2) RETURNING id",
                &[&file.name, &file.content],
            )
            .await
            .map_err(|source| query_error("insert initial file", source))?;
        let file_id: i64 = row.get(0);

        for &path in file.paths {
            transaction
                .execute(
                    "INSERT INTO public.paths (path, file_id) VALUES ($1, $2)",
                    &[&path, &file_id],
                )
                .await
                .map_err(|source| query_error("insert initial path", source))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn initial_content_has_nonempty_files_and_unique_paths() {
        let paths: HashSet<_> = INITIAL_FILES
            .iter()
            .flat_map(|file| file.paths.iter())
            .collect();

        assert_eq!(INITIAL_FILES.len(), 3);
        assert_eq!(paths.len(), 4);
        assert!(INITIAL_FILES.iter().all(|file| !file.name.is_empty()));
        assert!(INITIAL_FILES.iter().all(|file| !file.content.is_empty()));
    }

    #[test]
    fn index_is_available_at_the_root_and_direct_paths() {
        let index = INITIAL_FILES
            .iter()
            .find(|file| file.name == "index.html")
            .expect("index should be included in initial content");

        assert_eq!(index.paths, &["/", "/index.html"]);
    }
}
