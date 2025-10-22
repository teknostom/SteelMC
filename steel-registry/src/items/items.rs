use steel_utils::ResourceLocation;

use crate::{blocks::blocks::BlockRef, data_components::DataComponentMap, registry::{Registrable, Registry}};

#[derive(Debug)]
pub struct Item {
    pub key: ResourceLocation,
    pub components: DataComponentMap,
}

impl Item {
    pub fn from_block(block: BlockRef) -> Self {
        Self {
            key: block.key.clone(),
            components: DataComponentMap::common_item_components(),
        }
    }

    pub fn from_block_custom_name(_block: BlockRef, name: &'static str) -> Self {
        Self {
            key: ResourceLocation::vanilla_static(name),
            components: DataComponentMap::common_item_components(),
        }
    }
}

pub type ItemRef = &'static Item;

impl Registrable for Item {
    fn key(&self) -> &ResourceLocation {
        &self.key
    }
}

pub struct ItemRegistry {
    registry: Registry<Item>,
}

impl Default for ItemRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemRegistry {
    pub fn new() -> Self {
        Self {
            registry: Registry::new(),
        }
    }

    pub fn freeze(&mut self) {
        self.registry.freeze();
    }

    pub fn register(&mut self, item: ItemRef) -> usize {
        self.registry.register(item)
    }

    pub fn by_id(&self, id: usize) -> Option<ItemRef> {
        self.registry.by_id(id)
    }

    pub fn get_id(&self, item: ItemRef) -> &usize {
        self.registry.get_id(item)
    }

    pub fn by_key(&self, key: &ResourceLocation) -> Option<ItemRef> {
        self.registry.by_key(key)
    }
}
