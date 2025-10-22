#![feature(
    const_trait_impl,
    const_slice_make_iter,
    const_cmp,
    derive_const,
    core_intrinsics
)]
#![allow(internal_features)]
pub mod blocks;
pub mod data_components;
pub mod items;
pub mod registry;

//#[rustfmt::skip]
#[path = "generated/vanilla_blocks.rs"]
pub mod vanilla_blocks;

//#[rustfmt::skip]
#[path = "generated/vanilla_items.rs"]
pub mod vanilla_items;
