use zenapi::collections::{ApiCollection, CollectionItem, CollectionRequest};

use super::super::folders::{
    collection_folder_items_exists, collection_folder_items_mut, parse_collection_folder_path_key,
};
use super::queries::count_collection_requests;

pub(in crate::app) fn add_collection_request_in(
    collection: &mut ApiCollection,
    folder_path_key: &str,
    request: CollectionRequest,
) -> Option<i32> {
    let folder_path = parse_collection_folder_path_key(folder_path_key)?;
    if folder_path.is_empty() {
        let row_id = count_collection_requests(&collection.items) as i32;
        collection.items.push(CollectionItem::Request(request));
        return Some(row_id);
    }

    let mut current = 0usize;
    let mut request = Some(request);
    add_collection_request_to_folder_items(
        &mut collection.items,
        &folder_path,
        &mut current,
        &mut request,
    )
}

fn add_collection_request_to_folder_items(
    items: &mut Vec<CollectionItem>,
    folder_path: &[String],
    current: &mut usize,
    request: &mut Option<CollectionRequest>,
) -> Option<i32> {
    let (target, rest) = folder_path.split_first()?;
    for item in items {
        match item {
            CollectionItem::Folder(folder) if folder.name.as_str() == target.as_str() => {
                if rest.is_empty() {
                    let row_id = *current + count_collection_requests(&folder.items);
                    folder.items.push(CollectionItem::Request(request.take()?));
                    return Some(row_id as i32);
                }
                return add_collection_request_to_folder_items(
                    &mut folder.items,
                    rest,
                    current,
                    request,
                );
            }
            CollectionItem::Folder(folder) => {
                *current += count_collection_requests(&folder.items);
            }
            CollectionItem::Request(_) => {
                *current += 1;
            }
        }
    }
    None
}

pub(in crate::app) fn duplicate_collection_request_at(
    collection: &mut ApiCollection,
    index: usize,
) -> Option<CollectionRequest> {
    let mut current = 0;
    duplicate_collection_request_at_items(&mut collection.items, index, &mut current)
}

fn duplicate_collection_request_at_items(
    items: &mut Vec<CollectionItem>,
    target: usize,
    current: &mut usize,
) -> Option<CollectionRequest> {
    let mut position = 0;
    while position < items.len() {
        match &mut items[position] {
            CollectionItem::Request(request) => {
                if *current == target {
                    let mut duplicate = request.clone();
                    duplicate.name = format!("{} Copy", duplicate.name);
                    items.insert(position + 1, CollectionItem::Request(duplicate.clone()));
                    return Some(duplicate);
                }
                *current += 1;
            }
            CollectionItem::Folder(folder) => {
                if let Some(request) =
                    duplicate_collection_request_at_items(&mut folder.items, target, current)
                {
                    return Some(request);
                }
            }
        }
        position += 1;
    }
    None
}

pub(in crate::app) fn rename_collection_request_at(
    collection: &mut ApiCollection,
    index: usize,
    name: &str,
) -> Option<CollectionRequest> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let mut current = 0;
    rename_collection_request_at_items(&mut collection.items, index, &mut current, name)
}

fn rename_collection_request_at_items(
    items: &mut [CollectionItem],
    target: usize,
    current: &mut usize,
    name: &str,
) -> Option<CollectionRequest> {
    for item in items {
        match item {
            CollectionItem::Request(request) => {
                if *current == target {
                    request.name = name.to_string();
                    return Some(request.clone());
                }
                *current += 1;
            }
            CollectionItem::Folder(folder) => {
                if let Some(request) =
                    rename_collection_request_at_items(&mut folder.items, target, current, name)
                {
                    return Some(request);
                }
            }
        }
    }
    None
}

pub(in crate::app) fn remove_collection_request_at(
    collection: &mut ApiCollection,
    index: usize,
) -> Option<CollectionRequest> {
    let mut current = 0;
    remove_collection_request_at_items(&mut collection.items, index, &mut current)
}

fn remove_collection_request_at_items(
    items: &mut Vec<CollectionItem>,
    target: usize,
    current: &mut usize,
) -> Option<CollectionRequest> {
    let mut position = 0;
    while position < items.len() {
        if matches!(&items[position], CollectionItem::Request(_)) {
            if *current == target {
                let CollectionItem::Request(request) = items.remove(position) else {
                    unreachable!("collection item kind checked before removal");
                };
                return Some(request);
            }
            *current += 1;
            position += 1;
            continue;
        }

        if let CollectionItem::Folder(folder) = &mut items[position] {
            if let Some(request) =
                remove_collection_request_at_items(&mut folder.items, target, current)
            {
                return Some(request);
            }
        }
        position += 1;
    }
    None
}

pub(in crate::app) fn move_collection_request_to_folder(
    collection: &mut ApiCollection,
    index: usize,
    target_folder_path_key: &str,
) -> Option<CollectionRequest> {
    let target_path = parse_collection_folder_path_key(target_folder_path_key)?;
    if !collection_folder_items_exists(&collection.items, &target_path) {
        return None;
    }

    let request = remove_collection_request_at(collection, index)?;
    if target_path.is_empty() {
        collection
            .items
            .push(CollectionItem::Request(request.clone()));
        return Some(request);
    }

    let target_items = collection_folder_items_mut(&mut collection.items, &target_path)?;
    target_items.push(CollectionItem::Request(request.clone()));
    Some(request)
}

pub(in crate::app) fn reorder_collection_request_at(
    collection: &mut ApiCollection,
    index: usize,
    delta: i32,
) -> Option<(CollectionRequest, i32)> {
    if delta == 0 {
        return None;
    }

    let mut current = 0usize;
    reorder_collection_request_at_items(&mut collection.items, index, &mut current, delta)
}

fn reorder_collection_request_at_items(
    items: &mut Vec<CollectionItem>,
    target: usize,
    current: &mut usize,
    delta: i32,
) -> Option<(CollectionRequest, i32)> {
    let mut position = 0;
    while position < items.len() {
        if matches!(&items[position], CollectionItem::Request(_)) {
            if *current == target {
                let request = match &items[position] {
                    CollectionItem::Request(request) => request.clone(),
                    CollectionItem::Folder(_) => unreachable!("collection request kind checked"),
                };
                let (next_position, new_index) = if delta < 0 {
                    let next_position = position.checked_sub(1)?;
                    let skipped = collection_item_request_count(&items[next_position]);
                    (next_position, target.saturating_sub(skipped))
                } else {
                    let next_position = (position + 1 < items.len()).then_some(position + 1)?;
                    let skipped = collection_item_request_count(&items[next_position]);
                    (next_position, target + skipped)
                };
                items.swap(position, next_position);
                return Some((request, new_index as i32));
            }
            *current += 1;
            position += 1;
            continue;
        }

        if let CollectionItem::Folder(folder) = &mut items[position] {
            if let Some(result) =
                reorder_collection_request_at_items(&mut folder.items, target, current, delta)
            {
                return Some(result);
            }
        }
        position += 1;
    }
    None
}

fn collection_item_request_count(item: &CollectionItem) -> usize {
    match item {
        CollectionItem::Folder(folder) => count_collection_requests(&folder.items),
        CollectionItem::Request(_) => 1,
    }
}
