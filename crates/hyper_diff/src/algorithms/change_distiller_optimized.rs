use super::MappingDurations;
use super::{DiffResult, PreparedMappingDurations};
use crate::actions::action_vec::ActionsVec;
use crate::decompressed_tree_store::lazy_post_order::LazyPostOrder;
use crate::matchers::heuristic::cd::bottom_up_matcher::BottomUpMatcher;
use crate::matchers::heuristic::cd::leaves_matcher::LeavesMatcher;
use crate::matchers::heuristic::cd::optimized_bottom_up_matcher::OptimizedBottomUpMatcher;
use crate::matchers::heuristic::cd::optimized_leaves_matcher::OptimizedLeavesMatcher;
use crate::matchers::heuristic::cd::{
    BottomUpMatcherConfig, BottomUpMatcherMetrics, CDResult, LeavesMatcherConfig,
    LeavesMatcherMetrics, OptimizedBottomUpMatcherConfig, OptimizedDiffConfig,
    OptimizedLeavesMatcherConfig,
};
use crate::{
    actions::script_generator2::{ScriptGenerator, SimpleAction},
    decompressed_tree_store::{CompletePostOrder, bfs_wrapper::SimpleBfsMapper},
    matchers::{
        Decompressible, Mapper,
        mapping_store::{MappingStore, VecStore},
    },
    tree::tree_path::CompressedTreePath,
};
use hyperast::types::{self, HyperAST, HyperASTShared, NodeId};
use serde::Serialize;
use std::{
    fmt::Debug,
    time::{Duration, Instant},
};

type DS<HAST: HyperASTShared> = Decompressible<HAST, LazyPostOrder<HAST::IdN, u32>>;
type CDS<HAST: HyperASTShared> = Decompressible<HAST, CompletePostOrder<HAST::IdN, u32>>;

/// Main optimized diff function with full configuration control
pub fn diff_optimized<HAST: HyperAST + Copy>(
    hyperast: HAST,
    src: &HAST::IdN,
    dst: &HAST::IdN,
    config: OptimizedDiffConfig,
) -> DiffResult<
    SimpleAction<HAST::Label, CompressedTreePath<HAST::Idx>, HAST::IdN>,
    Mapper<HAST, CDS<HAST>, CDS<HAST>, VecStore<u32>>,
    PreparedMappingDurations<2>,
>
where
    HAST::IdN: Clone + Debug + Eq,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    HAST::Idx: hyperast::PrimInt,
    HAST::Label: Debug + Copy + Eq,
    for<'t> types::LendT<'t, HAST>: types::WithHashs + types::WithStats,
{
    diff_optimized_verbose(hyperast, src, dst, config).into_diff_result()
}

/// Verbose version that returns detailed metrics
pub fn diff_optimized_verbose<HAST: HyperAST + Copy>(
    hyperast: HAST,
    src: &HAST::IdN,
    dst: &HAST::IdN,
    config: OptimizedDiffConfig,
) -> CDResult<
    SimpleAction<HAST::Label, CompressedTreePath<HAST::Idx>, HAST::IdN>,
    Mapper<HAST, CDS<HAST>, CDS<HAST>, VecStore<u32>>,
    PreparedMappingDurations<2>,
>
where
    HAST::IdN: Clone + Debug + Eq,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    HAST::Idx: hyperast::PrimInt,
    HAST::Label: Debug + Copy + Eq,
    for<'t> types::LendT<'t, HAST>: types::WithHashs + types::WithStats,
{
    if config.use_lazy_decompression {
        diff_with_lazy_decompression_verbose(hyperast, src, dst, config)
    } else {
        diff_with_complete_decompression_verbose(hyperast, src, dst, config)
    }
}

/// Convenience function with all optimizations enabled
pub fn diff_with_all_optimizations<HAST: HyperAST + Copy>(
    hyperast: HAST,
    src: &HAST::IdN,
    dst: &HAST::IdN,
) -> DiffResult<
    SimpleAction<HAST::Label, CompressedTreePath<HAST::Idx>, HAST::IdN>,
    Mapper<HAST, CDS<HAST>, CDS<HAST>, VecStore<u32>>,
    PreparedMappingDurations<2>,
>
where
    HAST::IdN: Clone + Debug + Eq,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    HAST::Idx: hyperast::PrimInt,
    HAST::Label: Debug + Copy + Eq,
    for<'t> types::LendT<'t, HAST>: types::WithHashs + types::WithStats,
{
    diff_optimized(hyperast, src, dst, OptimizedDiffConfig::default())
}

/// Convenience function with baseline configuration (no optimizations)
pub fn diff_baseline<HAST: HyperAST + Copy>(
    hyperast: HAST,
    src: &HAST::IdN,
    dst: &HAST::IdN,
) -> DiffResult<
    SimpleAction<HAST::Label, CompressedTreePath<HAST::Idx>, HAST::IdN>,
    Mapper<HAST, CDS<HAST>, CDS<HAST>, VecStore<u32>>,
    PreparedMappingDurations<2>,
>
where
    HAST::IdN: Clone + Debug + Eq,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    HAST::Idx: hyperast::PrimInt,
    HAST::Label: Debug + Copy + Eq,
    for<'t> types::LendT<'t, HAST>: types::WithHashs + types::WithStats,
{
    let config = OptimizedDiffConfig::baseline();
    diff_optimized(hyperast, src, dst, config)
}

/// Execute diff with lazy decompression
fn diff_with_lazy_decompression<HAST: HyperAST + Copy>(
    hyperast: HAST,
    src: &HAST::IdN,
    dst: &HAST::IdN,
    config: OptimizedDiffConfig,
) -> DiffResult<
    SimpleAction<HAST::Label, CompressedTreePath<HAST::Idx>, HAST::IdN>,
    Mapper<HAST, CDS<HAST>, CDS<HAST>, VecStore<u32>>,
    PreparedMappingDurations<2>,
>
where
    HAST::IdN: Clone + Debug + Eq,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    HAST::Idx: hyperast::PrimInt,
    HAST::Label: Debug + Copy + Eq,
    for<'t> types::LendT<'t, HAST>: types::WithHashs + types::WithStats,
{
    diff_with_lazy_decompression_verbose(hyperast, src, dst, config).into_diff_result()
}

/// Execute diff with lazy decompression (verbose version)
fn diff_with_lazy_decompression_verbose<HAST: HyperAST + Copy>(
    hyperast: HAST,
    src: &HAST::IdN,
    dst: &HAST::IdN,
    config: OptimizedDiffConfig,
) -> CDResult<
    SimpleAction<HAST::Label, CompressedTreePath<HAST::Idx>, HAST::IdN>,
    Mapper<HAST, CDS<HAST>, CDS<HAST>, VecStore<u32>>,
    PreparedMappingDurations<2>,
>
where
    HAST::IdN: Clone + Debug + Eq,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    HAST::Idx: hyperast::PrimInt,
    HAST::Label: Debug + Copy + Eq,
    for<'t> types::LendT<'t, HAST>: types::WithHashs + types::WithStats,
{
    let start = Instant::now();
    log::debug!(
        "Starting Optimized ChangeDistiller Algorithm with lazy decompression. Preparing subtrees"
    );
    let now = Instant::now();

    let mapper: (HAST, (DS<HAST>, DS<HAST>)) = hyperast.decompress_pair(src, dst);
    let mut mapper_owned: Mapper<_, DS<HAST>, DS<HAST>, VecStore<_>> = mapper.into();

    let mapper = Mapper {
        hyperast,
        mapping: crate::matchers::Mapping {
            src_arena: mapper_owned.mapping.src_arena.as_mut(),
            dst_arena: mapper_owned.mapping.dst_arena.as_mut(),
            mappings: mapper_owned.mapping.mappings,
        },
    };
    let subtree_prepare_time = now.elapsed();
    let subtree_prepare_t = subtree_prepare_time.as_secs_f64();
    log::debug!("Subtree prepare time: {}", subtree_prepare_t);

    log::debug!("Starting OptimizedLeavesMatcher");
    let leaves_start = Instant::now();
    let (mapper, leaves_matcher_metrics) =
        OptimizedLeavesMatcher::with_config_and_metrics(mapper, config.leaves_matcher);
    let leaves_matcher_time = leaves_start.elapsed();
    let leaves_matcher_t = leaves_matcher_time.as_secs_f64();
    let leaves_mappings_s = mapper.mappings().len();
    log::debug!(
        "LeavesMatcher time: {}, Leaves mappings: {}",
        leaves_matcher_t,
        leaves_mappings_s
    );

    log::debug!("Starting OptimizedBottomUpMatcher");
    let bottomup_start = Instant::now();
    let (mapper, bottomup_matcher_metrics) =
        OptimizedBottomUpMatcher::with_config_and_metrics(mapper, config.bottom_up_matcher);
    let bottomup_matcher_time = bottomup_start.elapsed();
    let bottomup_matcher_t = bottomup_matcher_time.as_secs_f64();
    let bottomup_mappings_s = mapper.mappings().len();
    log::debug!(
        "Bottom-up matcher time: {}, Bottom-up mappings: {}",
        bottomup_matcher_t,
        bottomup_mappings_s
    );

    let (actions, prepare_gen_t, gen_t, mapper) = if config.calculate_script {
        log::debug!("Starting script generation");
        let now = Instant::now();

        let mapper = mapper.map(
            |x| x,
            |dst_arena| SimpleBfsMapper::with_store(hyperast, dst_arena),
        );
        let prepare_gen_t = now.elapsed().as_secs_f64();
        let now = Instant::now();
        let actions = ScriptGenerator::compute_actions(hyperast, &mapper.mapping).ok();
        let gen_t = now.elapsed().as_secs_f64();
        log::debug!("Script generator time: {}", gen_t);
        log::debug!("Prepare generator time: {}", prepare_gen_t);

        let mapper = Mapper {
            hyperast,
            mapping: crate::matchers::Mapping {
                mappings: mapper.mapping.mappings,
                src_arena: mapper_owned.mapping.src_arena,
                dst_arena: mapper_owned.mapping.dst_arena,
            },
        };
        let mapper = mapper.map(
            |src_arena| {
                Decompressible::<HAST, CompletePostOrder<HAST::IdN, _>>::from(
                    src_arena.map(|x| x.complete(hyperast)),
                )
            },
            |dst_arena| {
                let complete = Decompressible::<HAST, CompletePostOrder<HAST::IdN, _>>::from(
                    dst_arena.map(|x| x.complete(hyperast)),
                );
                SimpleBfsMapper::with_store(hyperast, complete)
            },
        );

        let mapper = mapper.map(|x| x, |dst_arena| dst_arena.back);
        (actions, prepare_gen_t, gen_t, mapper)
    } else {
        let mapper = Mapper {
            hyperast,
            mapping: crate::matchers::Mapping {
                mappings: mapper.mapping.mappings,
                src_arena: mapper_owned.mapping.src_arena,
                dst_arena: mapper_owned.mapping.dst_arena,
            },
        };
        let mapper = mapper.map(
            |src_arena| {
                Decompressible::<HAST, CompletePostOrder<HAST::IdN, _>>::from(
                    src_arena.map(|x| x.complete(hyperast)),
                )
            },
            |dst_arena| {
                let complete = Decompressible::<HAST, CompletePostOrder<HAST::IdN, _>>::from(
                    dst_arena.map(|x| x.complete(hyperast)),
                );
                SimpleBfsMapper::with_store(hyperast, complete)
            },
        );

        let mapper = mapper.map(|x| x, |dst_arena| dst_arena.back);
        (None, 0.0, 0.0, mapper)
    };

    let start = Instant::now();

    CDResult {
        total_time: start.elapsed(),
        mapping_durations: PreparedMappingDurations {
            mappings: MappingDurations([leaves_matcher_t, bottomup_matcher_t]),
            preparation: [subtree_prepare_t, 0.0],
        },
        mapper,
        actions,
        prepare_gen_t,
        gen_t,

        // Detailed timing metrics from actual measurements
        leaves_matcher_metrics,
        bottomup_matcher_metrics,

        produced_mappings: bottomup_mappings_s,
        subtree_prepare_time,
    }
}

/// Execute diff with complete decompression (baseline algorithm)
fn diff_with_complete_decompression<HAST: HyperAST + Copy>(
    hyperast: HAST,
    src: &HAST::IdN,
    dst: &HAST::IdN,
    config: OptimizedDiffConfig,
) -> DiffResult<
    SimpleAction<HAST::Label, CompressedTreePath<HAST::Idx>, HAST::IdN>,
    Mapper<HAST, CDS<HAST>, CDS<HAST>, VecStore<u32>>,
    PreparedMappingDurations<2>,
>
where
    HAST::IdN: Clone + Debug + Eq,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    HAST::Idx: hyperast::PrimInt,
    HAST::Label: Debug + Copy + Eq,
    for<'t> <HAST as hyperast::types::AstLending<'t>>::RT: types::WithHashs + types::WithStats,
{
    diff_with_complete_decompression_verbose(hyperast, src, dst, config).into_diff_result()
}

/// Execute diff with complete decompression (verbose baseline algorithm)
fn diff_with_complete_decompression_verbose<HAST: HyperAST + Copy>(
    hyperast: HAST,
    src: &HAST::IdN,
    dst: &HAST::IdN,
    config: OptimizedDiffConfig,
) -> CDResult<
    SimpleAction<HAST::Label, CompressedTreePath<HAST::Idx>, HAST::IdN>,
    Mapper<HAST, CDS<HAST>, CDS<HAST>, VecStore<u32>>,
    PreparedMappingDurations<2>,
>
where
    HAST::IdN: Clone + Debug + Eq,
    HAST::IdN: NodeId<IdN = HAST::IdN>,
    HAST::Idx: hyperast::PrimInt,
    HAST::Label: Debug + Copy + Eq,
    for<'t> <HAST as hyperast::types::AstLending<'t>>::RT: types::WithHashs + types::WithStats,
{
    let start = Instant::now();
    log::debug!(
        "Starting Optimized ChangeDistiller Algorithm with complete decompression. Preparing subtrees"
    );
    let now = Instant::now();
    let mapper: Mapper<_, CDS<HAST>, CDS<HAST>, VecStore<_>> =
        hyperast.decompress_pair(src, dst).into();
    let subtree_prepare_time = now.elapsed();
    let subtree_prepare_t = subtree_prepare_time.as_secs_f64();
    log::debug!("Subtree prepare time: {}", subtree_prepare_t);

    log::debug!("Starting LeavesMatcher (baseline)");
    let leaves_start = Instant::now();
    let (mapper, leaves_matcher_metrics) =
        LeavesMatcher::with_config_and_metrics(mapper, config.leaves_matcher);
    let leaves_matcher_time = leaves_start.elapsed();
    let leaves_matcher_t = leaves_matcher_time.as_secs_f64();
    let leaves_mappings_s = mapper.mappings().len();
    log::debug!(
        "LeavesMatcher time: {}, Leaves mappings: {}",
        leaves_matcher_t,
        leaves_mappings_s
    );

    log::debug!("Starting BottomUpMatcher (baseline)");
    let bottomup_start = Instant::now();
    let (mapper, bottomup_matcher_metrics) =
        BottomUpMatcher::with_config_and_metrics(mapper, config.bottom_up_matcher);
    let bottomup_matcher_time = bottomup_start.elapsed();
    let bottomup_matcher_t = bottomup_matcher_time.as_secs_f64();
    let bottomup_mappings_s = mapper.mappings().len();
    log::debug!(
        "Bottom-up matcher time: {}, Bottom-up mappings: {}",
        bottomup_matcher_t,
        bottomup_mappings_s
    );

    let (actions, prepare_gen_t, gen_t, mapper) = if config.calculate_script {
        log::debug!("Starting script generation");
        let now = Instant::now();

        let mapper = mapper.map(
            |x| x,
            |dst_arena| SimpleBfsMapper::with_store(hyperast, dst_arena),
        );
        let prepare_gen_t = now.elapsed().as_secs_f64();
        let now = Instant::now();
        let actions = ScriptGenerator::compute_actions(hyperast, &mapper.mapping).ok();
        let gen_t = now.elapsed().as_secs_f64();
        log::debug!("Script generator time: {}", gen_t);
        log::debug!("Prepare generator time: {}", prepare_gen_t);
        let mapper = mapper.map(|x| x, |dst_arena| dst_arena.back);
        (actions, prepare_gen_t, gen_t, mapper)
    } else {
        (None, 0.0, 0.0, mapper)
    };

    let total_time = start.elapsed();

    CDResult {
        mapping_durations: PreparedMappingDurations {
            mappings: MappingDurations([leaves_matcher_t, bottomup_matcher_t]),
            preparation: [subtree_prepare_t, 0.0],
        },
        mapper,
        actions,
        prepare_gen_t,
        gen_t,
        total_time,

        // Detailed timing metrics - baseline algorithm has limited metrics
        leaves_matcher_metrics,

        bottomup_matcher_metrics,

        produced_mappings: bottomup_mappings_s,
        subtree_prepare_time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::examples::example_simple;
    use crate::tree::simple_tree::vpair_to_stores;

    #[test]
    fn test_optimized_diff_all_optimizations() {
        let (stores, src, dst) = vpair_to_stores(example_simple());

        let result = diff_with_all_optimizations(&stores, &src, &dst);

        assert!(result.mapper.mappings().len() > 0);
        assert!(result.actions.is_some());
    }

    #[test]
    fn test_optimized_diff_baseline() {
        let (stores, src, dst) = vpair_to_stores(example_simple());

        let result = diff_baseline(&stores, &src, &dst);

        assert!(result.mapper.mappings().len() > 0);
        assert!(result.actions.is_some());
    }

    #[test]
    fn test_optimized_diff_custom_config() {
        let (stores, src, dst) = vpair_to_stores(example_simple());

        let config = OptimizedDiffConfig {
            use_lazy_decompression: true,
            use_ranged_similarity: true,
            calculate_script: false,
            leaves_matcher: OptimizedLeavesMatcherConfig {
                base_config: LeavesMatcherConfig::default(),
                enable_label_caching: true,
                enable_deep_leaves: false,
                enable_ngram_caching: false,
                enable_type_grouping: false,
                use_binary_heap: true,
                statement_level_iteration: true,
                reuse_qgram_object: false,
            },
            bottom_up_matcher: OptimizedBottomUpMatcherConfig {
                base: BottomUpMatcherConfig::default(),
                enable_type_grouping: true,
                enable_deep_leaves: false,
                statement_level_iteration: false,
                enable_leaf_count_precomputation: false,
            },
        };

        let result = diff_optimized(&stores, &src, &dst, config);

        assert!(result.mapper.mappings().len() > 0);
        assert!(result.actions.is_none()); // Script generation disabled
    }
}
