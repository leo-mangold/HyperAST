use crate::{
    decompressed_tree_store::{
        ContiguousDescendants, DecompressedTreeStore, DecompressedWithParent, LazyDecompressed,
        LazyDecompressedTreeStore, LazyPOBorrowSlice, PostOrder, PostOrderIterable, Shallow,
        ShallowDecompressedTreeStore,
    },
    matchers::mapping_store::MonoMappingStore,
};
use hyperast::PrimInt;
use hyperast::types::{HyperAST, LabelStore, Labeled, NodeId, NodeStore, WithHashs};
use std::cmp::Ordering;
use std::fmt::Debug;
use str_distance::DistanceMetric;

use super::LeavesMatcherConfig;

struct MappingWithSimilarity<M: MonoMappingStore> {
    src: M::Src,
    dst: M::Dst,
    sim: f64,
}

pub struct LazyLeavesMatcher<Dsrc, Ddst, HAST, M> {
    pub(super) stores: HAST,
    pub src_arena: Dsrc,
    pub dst_arena: Ddst,
    pub mappings: M,
    pub config: LeavesMatcherConfig,
}

impl<
    Dsrc: LazyDecompressed<M::Src>,
    Ddst: LazyDecompressed<M::Dst>,
    HAST: HyperAST + Copy,
    M: MonoMappingStore,
> LazyLeavesMatcher<Dsrc, Ddst, HAST, M>
where
    for<'t> <HAST as hyperast::types::AstLending<'t>>::RT: WithHashs,
    M::Src: PrimInt,
    M::Dst: PrimInt,
    HAST::Label: Eq,
    HAST::IdN: Debug,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    Dsrc: DecompressedTreeStore<HAST, Dsrc::IdD, M::Src>
        + DecompressedWithParent<HAST, Dsrc::IdD>
        + PostOrder<HAST, Dsrc::IdD, M::Src>
        + PostOrderIterable<HAST, Dsrc::IdD, M::Src>
        + ContiguousDescendants<HAST, Dsrc::IdD, M::Src>
        + LazyPOBorrowSlice<HAST, Dsrc::IdD, M::Src>
        + ShallowDecompressedTreeStore<HAST, Dsrc::IdD, M::Src>
        + LazyDecompressedTreeStore<HAST, M::Src>,
    Ddst: DecompressedTreeStore<HAST, Ddst::IdD, M::Dst>
        + DecompressedWithParent<HAST, Ddst::IdD>
        + PostOrder<HAST, Ddst::IdD, M::Dst>
        + PostOrderIterable<HAST, Ddst::IdD, M::Dst>
        + ContiguousDescendants<HAST, Ddst::IdD, M::Dst>
        + LazyPOBorrowSlice<HAST, Ddst::IdD, M::Dst>
        + ShallowDecompressedTreeStore<HAST, Ddst::IdD, M::Dst>
        + LazyDecompressedTreeStore<HAST, M::Dst>,
{
    pub fn with_config(
        mapping: crate::matchers::Mapper<HAST, Dsrc, Ddst, M>,
        config: LeavesMatcherConfig,
    ) -> crate::matchers::Mapper<HAST, Dsrc, Ddst, M> {
        let mut matcher = Self {
            stores: mapping.hyperast,
            src_arena: mapping.mapping.src_arena,
            dst_arena: mapping.mapping.dst_arena,
            mappings: mapping.mapping.mappings,
            config,
        };
        // Rest of the code remains the same
        matcher
            .mappings
            .topit(matcher.src_arena.len(), matcher.dst_arena.len());
        matcher.execute();
        // Return the mapper
        crate::matchers::Mapper {
            hyperast: mapping.hyperast,
            mapping: crate::matchers::Mapping {
                src_arena: matcher.src_arena,
                dst_arena: matcher.dst_arena,
                mappings: matcher.mappings,
            },
        }
    }

    pub fn match_it(
        mapping: crate::matchers::Mapper<HAST, Dsrc, Ddst, M>,
    ) -> crate::matchers::Mapper<HAST, Dsrc, Ddst, M> {
        Self::with_config(mapping, LeavesMatcherConfig::default())
    }

    fn execute(&mut self) {
        let dst_leaves: Vec<M::Dst> = self
            .dst_arena
            .iter_df_post::<true>()
            .filter(|t| {
                let id = self.dst_arena.decompress_to(&t);
                self.dst_arena.children(&id).is_empty()
            })
            .collect();

        let src_leaves: Vec<M::Src> = self
            .src_arena
            .iter_df_post::<true>()
            .filter(|t| {
                let id = self.src_arena.decompress_to(&t);
                self.src_arena.children(&id).is_empty()
            })
            .collect();

        let mut leaves_mappings: Vec<MappingWithSimilarity<M>> = Vec::new();

        for &src_leaf in &src_leaves {
            let src = self.src_arena.decompress_to(&src_leaf);

            for &dst_leaf in &dst_leaves {
                let dst = self.dst_arena.decompress_to(&dst_leaf);

                if self.is_mapping_allowed(&src, &dst) {
                    let sim = self.compute_label_similarity(&src, &dst);
                    if sim > self.config.label_sim_threshold {
                        leaves_mappings.push(MappingWithSimilarity {
                            src: src_leaf,
                            dst: dst_leaf,
                            sim,
                        });
                    }
                }
            }
        }

        // Sort mappings by similarity
        leaves_mappings.sort_by(|a, b| b.sim.partial_cmp(&a.sim).unwrap_or(Ordering::Equal));

        // Process mappings in order
        for mapping in leaves_mappings {
            self.mappings
                .link_if_both_unmapped(mapping.src, mapping.dst);
        }
    }

    fn is_mapping_allowed(&self, src_tree: &Dsrc::IdD, dst_tree: &Ddst::IdD) -> bool {
        let src_linked = self.mappings.is_src(src_tree.shallow());
        let dst_linked = self.mappings.is_dst(dst_tree.shallow());

        if src_linked || dst_linked {
            return false;
        }

        let original_src = self.src_arena.original(src_tree);
        let original_dst = self.dst_arena.original(dst_tree);

        let src_type = self.stores.resolve_type(&original_src);
        let dst_type = self.stores.resolve_type(&original_dst);

        src_type == dst_type
    }

    fn compute_label_similarity(&self, src_tree: &Dsrc::IdD, dst_tree: &Ddst::IdD) -> f64 {
        let original_src = self.src_arena.original(src_tree);
        let original_dst = self.dst_arena.original(dst_tree);

        let src_node = self.stores.node_store().resolve(&original_src);
        let dst_node = self.stores.node_store().resolve(&original_dst);

        let src_label_id = src_node.try_get_label();
        let dst_label_id = dst_node.try_get_label();

        match (src_label_id, dst_label_id) {
            (Some(src_label_id), Some(dst_label_id)) => {
                let src_label = self.stores.label_store().resolve(&src_label_id);
                let dst_label = self.stores.label_store().resolve(&dst_label_id);
                let dist =
                    str_distance::QGram::new(3).normalized(src_label.chars(), dst_label.chars());
                1.0 - dist
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompressed_tree_store::lazy_post_order::LazyPostOrder;
    use crate::matchers::Decompressible;
    use crate::matchers::mapping_store::MappingStore;
    use crate::matchers::{Mapper, mapping_store::DefaultMappingStore};
    use crate::tree::simple_tree::vpair_to_stores;
    use hyperast::nodes::SyntaxSerializer;
    use hyperast::test_utils::simple_tree::DisplayTree;
    use hyperast::types::{DecompressedFrom, WithChildren};

    #[test]
    fn test_leaves_matcher() {
        let (stores, src, dst) = vpair_to_stores(crate::tests::examples::example_leaf_label_swap());

        println!(
            "Src Tree:\n{}",
            DisplayTree::new(&stores.label_store, &stores.node_store, src)
        );

        println!(
            "Dst Tree:\n{}",
            DisplayTree::new(&stores.label_store, &stores.node_store, dst)
        );
        println!("Src Tree:\n{}", SyntaxSerializer::new(&stores, src));
        println!("Dst Tree:\n{}", SyntaxSerializer::new(&stores, dst));

        let mut src_arena = Decompressible::<_, LazyPostOrder<_, u16>>::decompress(&stores, &src);
        let mut dst_arena = Decompressible::<_, LazyPostOrder<_, u16>>::decompress(&stores, &dst);

        let mapping = Mapper {
            hyperast: &stores,
            mapping: crate::matchers::Mapping {
                src_arena: src_arena.as_mut(),
                dst_arena: dst_arena.as_mut(),
                mappings: DefaultMappingStore::default(),
            },
        };

        let result = LazyLeavesMatcher::match_it(mapping);

        assert_eq!(2, result.mappings.len());
        println!("Mappings: {:?}", result.mappings);
        assert!(HyperAST::resolve(&stores, &dst).child(&0).is_some());
        assert!(HyperAST::resolve(&stores, &dst).child(&1).is_some());
        assert!(HyperAST::resolve(&stores, &src).child(&0).is_some());
        assert!(HyperAST::resolve(&stores, &src).child(&1).is_some());

        println!(
            "Src Children: {:?}",
            HyperAST::resolve(&stores, &src).children()
        );
        println!(
            "Dst Children: {:?}",
            HyperAST::resolve(&stores, &dst).children()
        );

        use crate::decompressed_tree_store::ShallowDecompressedTreeStore;
        let src = result.mapping.src_arena.root();
        let src_cs = result.mapping.src_arena.children(&src);
        let dst = result.mapping.dst_arena.root();
        let dst_cs = result.mapping.dst_arena.children(&dst);

        assert!(result.mapping.mappings.has(&src_cs[0], &dst_cs[1]));
        assert!(result.mapping.mappings.has(&src_cs[1], &dst_cs[0]));
    }
}
