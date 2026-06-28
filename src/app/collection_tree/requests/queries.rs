use zenapi::collections::{ApiCollection, CollectionItem, CollectionRequest};

pub(in crate::app) fn count_collection_requests(items: &[CollectionItem]) -> usize {
    items
        .iter()
        .map(|item| match item {
            CollectionItem::Folder(folder) => count_collection_requests(&folder.items),
            CollectionItem::Request(_) => 1,
        })
        .sum()
}

pub(in crate::app) fn collection_request_at(
    collection: &ApiCollection,
    index: usize,
) -> Option<&CollectionRequest> {
    let mut current = 0;
    collection_request_at_items(&collection.items, index, &mut current)
}

fn collection_request_at_items<'a>(
    items: &'a [CollectionItem],
    target: usize,
    current: &mut usize,
) -> Option<&'a CollectionRequest> {
    for item in items {
        match item {
            CollectionItem::Folder(folder) => {
                if let Some(request) = collection_request_at_items(&folder.items, target, current) {
                    return Some(request);
                }
            }
            CollectionItem::Request(request) => {
                if *current == target {
                    return Some(request);
                }
                *current += 1;
            }
        }
    }
    None
}
