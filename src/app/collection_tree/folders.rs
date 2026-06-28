use zenapi::collections::{ApiCollection, CollectionFolder, CollectionItem};

pub(in crate::app) fn add_collection_folder_in(
    collection: &mut ApiCollection,
    parent_path_key: &str,
    name: &str,
) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let folder = CollectionItem::Folder(CollectionFolder {
        name: name.to_string(),
        description: String::new(),
        items: Vec::new(),
    });

    let parent_path = parse_collection_folder_path_key(parent_path_key)?;
    if parent_path.is_empty() {
        collection.items.push(folder);
        return Some(name.to_string());
    }

    let parent = collection_folder_items_mut(&mut collection.items, &parent_path)?;
    parent.push(folder);

    Some(name.to_string())
}

pub(in crate::app) fn rename_collection_folder_at(
    collection: &mut ApiCollection,
    folder_path_key: &str,
    name: &str,
) -> Option<(String, String)> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let mut folder_path = parse_collection_folder_path_key(folder_path_key)?;
    let folder_name = folder_path.pop()?;
    let parent_items = if folder_path.is_empty() {
        &mut collection.items
    } else {
        collection_folder_items_mut(&mut collection.items, &folder_path)?
    };

    let folder = parent_items.iter_mut().find_map(|item| match item {
        CollectionItem::Folder(folder) if folder.name == folder_name => Some(folder),
        _ => None,
    })?;
    folder.name = name.to_string();

    folder_path.push(name.to_string());
    Some((
        folder.name.clone(),
        collection_folder_path_key(&folder_path),
    ))
}

pub(in crate::app) fn remove_collection_folder_at(
    collection: &mut ApiCollection,
    folder_path_key: &str,
) -> Option<CollectionFolder> {
    let folder_path = parse_collection_folder_path_key(folder_path_key)?;
    if folder_path.is_empty() {
        return None;
    }

    remove_collection_folder_at_items(&mut collection.items, &folder_path)
}

fn remove_collection_folder_at_items(
    items: &mut Vec<CollectionItem>,
    folder_path: &[String],
) -> Option<CollectionFolder> {
    let (target, rest) = folder_path.split_first()?;
    let position = items.iter().position(|item| {
        matches!(item, CollectionItem::Folder(folder) if folder.name.as_str() == target.as_str())
    })?;

    if rest.is_empty() {
        let CollectionItem::Folder(folder) = items.remove(position) else {
            unreachable!("collection folder kind checked before removal");
        };
        return Some(folder);
    }

    let CollectionItem::Folder(folder) = &mut items[position] else {
        unreachable!("collection folder kind checked before recursion");
    };
    remove_collection_folder_at_items(&mut folder.items, rest)
}

pub(in crate::app) fn reorder_collection_folder_at(
    collection: &mut ApiCollection,
    folder_path_key: &str,
    delta: i32,
) -> Option<(String, String)> {
    if delta == 0 {
        return None;
    }

    let mut folder_path = parse_collection_folder_path_key(folder_path_key)?;
    let folder_name = folder_path.pop()?;
    let parent_items = if folder_path.is_empty() {
        &mut collection.items
    } else {
        collection_folder_items_mut(&mut collection.items, &folder_path)?
    };

    let position = parent_items.iter().position(|item| {
        matches!(item, CollectionItem::Folder(folder) if folder.name.as_str() == folder_name.as_str())
    })?;
    let next_position = if delta < 0 {
        position.checked_sub(1)?
    } else {
        (position + 1 < parent_items.len()).then_some(position + 1)?
    };
    parent_items.swap(position, next_position);

    folder_path.push(folder_name.clone());
    Some((folder_name, collection_folder_path_key(&folder_path)))
}

pub(super) fn collection_folder_items_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    folder_path: &[String],
) -> Option<&'a mut Vec<CollectionItem>> {
    let (current, rest) = folder_path.split_first()?;
    let folder = items.iter_mut().find_map(|item| match item {
        CollectionItem::Folder(folder) if folder.name.as_str() == current.as_str() => Some(folder),
        _ => None,
    })?;

    if rest.is_empty() {
        Some(&mut folder.items)
    } else {
        collection_folder_items_mut(&mut folder.items, rest)
    }
}

pub(super) fn collection_folder_items_exists(
    items: &[CollectionItem],
    folder_path: &[String],
) -> bool {
    if folder_path.is_empty() {
        return true;
    }

    let Some((current, rest)) = folder_path.split_first() else {
        return true;
    };
    let Some(folder) = items.iter().find_map(|item| match item {
        CollectionItem::Folder(folder) if folder.name.as_str() == current.as_str() => Some(folder),
        _ => None,
    }) else {
        return false;
    };

    collection_folder_items_exists(&folder.items, rest)
}

pub(super) fn collection_folder_path_key(folder_path: &[String]) -> String {
    serde_json::to_string(folder_path).unwrap_or_else(|_| "[]".to_string())
}

pub(super) fn parse_collection_folder_path_key(folder_path_key: &str) -> Option<Vec<String>> {
    let trimmed = folder_path_key.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }
    serde_json::from_str(trimmed).ok()
}

pub(in crate::app) fn collection_folder_label(folder_path_key: &str) -> String {
    let Some(path) = parse_collection_folder_path_key(folder_path_key) else {
        return "Collection".to_string();
    };
    if path.is_empty() {
        "Collection".to_string()
    } else {
        path.join(" / ")
    }
}
