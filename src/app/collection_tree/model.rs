use slint::{ModelRc, VecModel};
use zenapi::collections::{ApiCollection, CollectionItem};

use crate::ui::CollectionRow;

use super::folders::collection_folder_path_key;

pub(in crate::app) fn collection_model(collection: &ApiCollection) -> ModelRc<CollectionRow> {
    let mut rows = Vec::new();
    let mut next_id = 0;
    collect_collection_rows(&collection.items, 0, &[], &mut next_id, &mut rows);
    ModelRc::new(VecModel::from_iter(rows))
}

fn collect_collection_rows(
    items: &[CollectionItem],
    depth: usize,
    folder_path: &[String],
    next_id: &mut i32,
    rows: &mut Vec<CollectionRow>,
) {
    for item in items {
        match item {
            CollectionItem::Folder(folder) => {
                let mut current_path = folder_path.to_vec();
                current_path.push(folder.name.clone());
                rows.push(CollectionRow {
                    id: -1,
                    method: String::new().into(),
                    name: indented_collection_label(depth, &folder.name).into(),
                    url: String::new().into(),
                    is_folder: true,
                    folder_path: collection_folder_path_key(&current_path).into(),
                });
                collect_collection_rows(&folder.items, depth + 1, &current_path, next_id, rows);
            }
            CollectionItem::Request(request) => {
                rows.push(CollectionRow {
                    id: *next_id,
                    method: request.method.clone().into(),
                    name: indented_collection_label(depth, &request.name).into(),
                    url: request.url.clone().into(),
                    is_folder: false,
                    folder_path: String::new().into(),
                });
                *next_id += 1;
            }
        }
    }
}

fn indented_collection_label(depth: usize, name: &str) -> String {
    format!("{}{}", "  ".repeat(depth), name)
}
