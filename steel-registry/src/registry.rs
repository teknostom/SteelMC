use std::collections::HashMap;
use steel_utils::ResourceLocation;

/// A trait for types that can be registered in a Registry.
pub trait Registrable {
    /// Returns the resource location key for this registrable item.
    fn key(&self) -> &ResourceLocation;
}

/// A generic registry for managing items of type T.
pub struct Registry<T: Registrable + 'static> {
    items_by_id: Vec<&'static T>,
    items_by_key: HashMap<ResourceLocation, usize>,
    allows_registering: bool,
}

impl<T: Registrable + 'static> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Registrable + 'static> Registry<T> {
    /// Creates a new, empty registry.
    pub fn new() -> Self {
        Self {
            items_by_id: Vec::new(),
            items_by_key: HashMap::new(),
            allows_registering: true,
        }
    }

    /// Prevents the registry from registering new items.
    pub fn freeze(&mut self) {
        self.allows_registering = false;
    }

    /// Registers a new item and returns its ID.
    pub fn register(&mut self, item: &'static T) -> usize {
        if !self.allows_registering {
            panic!("Cannot register items after the registry has been frozen");
        }

        let id = self.items_by_id.len();
        self.items_by_key.insert(item.key().clone(), id);
        self.items_by_id.push(item);

        id
    }

    /// Retrieves an item by its ID.
    pub fn by_id(&self, id: usize) -> Option<&'static T> {
        self.items_by_id.get(id).copied()
    }

    /// Gets the ID for a registered item.
    pub fn get_id(&self, item: &'static T) -> &usize {
        self.items_by_key.get(item.key()).expect("Item not found")
    }

    /// Retrieves an item by its resource location key.
    pub fn by_key(&self, key: &ResourceLocation) -> Option<&'static T> {
        self.items_by_key.get(key).and_then(|id| self.by_id(*id))
    }
}
