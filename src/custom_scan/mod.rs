#![allow(non_snake_case)]
use pgx::nodes::node_to_string;
use pgx::*;

struct CustomScanGlobalState {
    prev_set_rel_pathlist_hook: pg_sys::set_rel_pathlist_hook_type,
}

static mut GLOBAL_STATE: CustomScanGlobalState = CustomScanGlobalState {
    prev_set_rel_pathlist_hook: None,
};

#[cfg(feature = "pg10")]
const CUSTOM_PATH_METHODS: pg_sys::CustomPathMethods = pg_sys::CustomPathMethods {
    CustomName: b"ZomboDB Custom Path\0".as_ptr() as *const i8,
    PlanCustomPath: Some(PlanCustomPath),
};

#[cfg(any(feature = "pg11", feature = "pg12"))]
const CUSTOM_PATH_METHODS: pg_sys::CustomPathMethods = pg_sys::CustomPathMethods {
    CustomName: b"ZomboDB Custom Path\0".as_ptr() as *const i8,
    PlanCustomPath: Some(PlanCustomPath),
    ReparameterizeCustomPathByChild: None,
};

const CUSTOM_SCAN_METHODS: pg_sys::CustomScanMethods = pg_sys::CustomScanMethods {
    CustomName: b"ZomboDB Custom Scan\0".as_ptr() as *const i8,
    CreateCustomScanState: Some(CreateCustomScanState),
};

const CUSTOM_EXEC_METHODS: pg_sys::CustomExecMethods = pg_sys::CustomExecMethods {
    CustomName: b"ZomboDB Exec\0".as_ptr() as *const i8,
    BeginCustomScan: Some(BeginCustomScan),
    ExecCustomScan: Some(ExecCustomScan),
    EndCustomScan: Some(EndCustomScan),
    ReScanCustomScan: Some(ReScanCustomScan),
    MarkPosCustomScan: None,
    RestrPosCustomScan: None,
    EstimateDSMCustomScan: None,
    InitializeDSMCustomScan: None,
    ReInitializeDSMCustomScan: None,
    InitializeWorkerCustomScan: None,
    ShutdownCustomScan: None,
    ExplainCustomScan: Some(ExplainCustomScan),
};

#[repr(C)]
struct ZDBScanState {
    custom_scan_state: pg_sys::CustomScanState,
}

/// initialize this module which requires saving the current `pg_sys::set_rel_pathlist_hook`
/// and installing our own
pub(crate) unsafe fn init() {
    //    // save the existing hook in our global state
    //    GLOBAL_STATE.prev_set_rel_pathlist_hook = pg_sys::set_rel_pathlist_hook;
    //
    //    // and install our own
    //    pg_sys::set_rel_pathlist_hook = Some(pathlist_hook);
}

/// Although this hook function can be used to examine, modify, or remove paths generated by the
/// core system, a custom scan provider will typically confine itself to generating CustomPath
/// objects and adding them to rel using `add_path`. The custom scan provider is responsible for
/// initializing the CustomPath object
///
/// `path` must be initialized as for any other path, including the row-count estimate, start and
/// total cost, and sort ordering provided by this path. flags is a bit mask, which should include
/// `CUSTOMPATH_SUPPORT_BACKWARD_SCAN` if the custom path can support a backward scan and
/// `CUSTOMPATH_SUPPORT_MARK_RESTORE` if it can support mark and restore. Both capabilities are
/// optional. An optional `custom_paths` is a list of Path nodes used by this custom-path node;
/// these will be transformed into `Plan` nodes by planner. `custom_private` can be used to store
/// the custom path's private data. Private data should be stored in a form that can be handled by
/// nodeToString, so that debugging routines that attempt to print the custom path will work as
/// designed. `methods` must point to a (usually statically allocated) object implementing the
/// required custom path methods, of which there is currently only one.
#[pg_guard]
unsafe extern "C" fn pathlist_hook(
    root: *mut pg_sys::PlannerInfo,
    rel: *mut pg_sys::RelOptInfo,
    rti: pg_sys::Index,
    rte: *mut pg_sys::RangeTblEntry,
) {
    info!("called our pathlist_hook");

    // call the previous hook, if there is one
    match &mut GLOBAL_STATE.prev_set_rel_pathlist_hook {
        Some(prev_hook) => prev_hook(root, rel, rti, rte),
        None => {}
    }

    let zdbquery_oid = pg_sys::TypenameGetTypid(
        std::ffi::CStr::from_bytes_with_nul_unchecked(b"zdbquery\0").as_ptr(),
    );
    let zdb_operator_oid = PgQualifiedNameBuilder::new()
        .push("pg_catalog")
        .push("==>")
        .get_operator_oid(pg_sys::ANYELEMENTOID, zdbquery_oid);

    info!("type_oid={}, op_oid={}", zdbquery_oid, zdb_operator_oid);
    let rel = PgBox::from_pg(rel);
    let restrict_info_list = PgList::<pg_sys::RestrictInfo>::from_pg(rel.baserestrictinfo);

    for i in 0..restrict_info_list.len() {
        let ri = PgBox::<pg_sys::RestrictInfo>::from_pg(restrict_info_list.get(i).unwrap());

        info!(
            "ri={}",
            node_to_string(ri.as_ptr() as *mut pg_sys::Node).expect("got a null Node")
        );
    }

    let mut custom_path = PgNodeFactory::makeCustomPath();

    custom_path.path.type_ = pg_sys::NodeTag_T_CustomPath;

    custom_path.path.pathtype = pg_sys::NodeTag_T_CustomScan;
    custom_path.path.parent = rel.as_ptr();
    custom_path.path.pathtarget = rel.reltarget;
    custom_path.path.param_info =
        pg_sys::get_baserel_parampathinfo(root, rel.as_ptr(), rel.lateral_relids);

    custom_path.flags = 0;
    custom_path.methods = &CUSTOM_PATH_METHODS;

    //    pg_sys::add_path(rel.as_ptr(), custom_path.into_pg() as *mut pg_sys::Path);
    //    info!("called add_path");
}

/// Convert a custom path to a finished plan. The return value will generally be a CustomScan object,
/// which the callback must allocate and initialize.
#[pg_guard]
unsafe extern "C" fn PlanCustomPath(
    _root: *mut pg_sys::PlannerInfo,
    rel: *mut pg_sys::RelOptInfo,
    best_path: *mut pg_sys::CustomPath,
    tlist: *mut pg_sys::List,
    _clauses: *mut pg_sys::List,
    _custom_plans: *mut pg_sys::List,
) -> *mut pg_sys::Plan {
    info!("in PlanCustomPath");
    let rel = PgBox::from_pg(rel);
    let best_path = PgBox::from_pg(best_path);

    let mut custom_scan = PgNodeFactory::makeCustomScan();

    custom_scan.flags = best_path.flags;
    custom_scan.methods = &CUSTOM_SCAN_METHODS;

    custom_scan.scan.scanrelid = rel.relid;
    custom_scan.scan.plan.targetlist = tlist;
    //    custom_scan.scan.plan.qual = pg_sys::extract_actual_clauses(clauses, false);

    info!("finished PlanCustomPath");
    let ptr = custom_scan.into_pg();
    &mut ptr.as_mut().unwrap().scan.plan
}

/// Allocate a `CustomScanState` for this `CustomScan`. The actual allocation will often be larger than
/// required for an ordinary CustomScanState, because many providers will wish to embed that as the
/// first field of a larger structure. The value returned must have the node tag and methods set
/// appropriately, but other fields should be left as zeroes at this stage; after
/// `ExecInitCustomScan` performs basic initialization, the `BeginCustomScan` callback will be
/// invoked to give the custom scan provider a chance to do whatever else is needed.
#[pg_guard]
unsafe extern "C" fn CreateCustomScanState(cscan: *mut pg_sys::CustomScan) -> *mut pg_sys::Node {
    info!("in CreateCustomScanState");
    let cscan = PgBox::from_pg(cscan);
    let mut state = PgBox::<ZDBScanState>::alloc0();
    let state_as_node = state.as_ptr() as *mut pg_sys::Node;
    state_as_node.as_mut().unwrap().type_ = pg_sys::NodeTag_T_CustomScanState;

    state.custom_scan_state.flags = cscan.flags;
    state.custom_scan_state.methods = &CUSTOM_EXEC_METHODS;

    state.into_pg() as *mut pg_sys::Node
}

/// Complete initialization of the supplied `CustomScanState`. Standard fields have been initialized
/// by `ExecInitCustomScan`, but any private fields should be initialized here.
#[pg_guard]
unsafe extern "C" fn BeginCustomScan(
    _node: *mut pg_sys::CustomScanState,
    _estate: *mut pg_sys::EState,
    _eflags: i32,
) {
    info!("in BeginCustomScan");
}

/// Fetch the next scan tuple. If any tuples remain, it should fill `ps_ResultTupleSlot` with the next
/// tuple in the current scan direction, and then return the tuple slot. If not, NULL or an empty
/// slot should be returned.
#[pg_guard]
unsafe extern "C" fn ExecCustomScan(
    _node: *mut pg_sys::CustomScanState,
) -> *mut pg_sys::TupleTableSlot {
    info!("in ExecCustomScan");
    std::ptr::null_mut()
}

/// Clean up any private data associated with the CustomScanState. This method is required, but it
/// does not need to do anything if there is no associated data or it will be cleaned up
/// automatically.
#[pg_guard]
unsafe extern "C" fn EndCustomScan(_node: *mut pg_sys::CustomScanState) {
    info!("in EndCustomScan");
}

/// Rewind the current scan to the beginning and prepare to rescan the relation.
#[pg_guard]
unsafe extern "C" fn ReScanCustomScan(_node: *mut pg_sys::CustomScanState) {}

/// Save the current scan position so that it can subsequently be restored by the `RestrPosCustomScan`
/// callback. This callback is optional, and need only be supplied if the
/// `CUSTOMPATH_SUPPORT_MARK_RESTORE` flag is set.
#[pg_guard]
unsafe extern "C" fn MarkPosCustomScan(_node: *mut pg_sys::CustomScanState) {}

/// Restore the previous scan position as saved by the `MarkPosCustomScan` callback. This callback is
/// optional, and need only be supplied if the `CUSTOMPATH_SUPPORT_MARK_RESTORE` flag is set.
#[pg_guard]
unsafe extern "C" fn RestrPosCustomScan(_node: *mut pg_sys::CustomScanState) {}

/// Estimate the amount of dynamic shared memory that will be required for parallel operation. This
/// may be higher than the amount that will actually be used, but it must not be lower. The return
/// value is in bytes. This callback is optional, and need only be supplied if this custom scan
/// provider supports parallel execution.
#[pg_guard]
unsafe extern "C" fn EstimateDSMCustomScan(
    _node: *mut pg_sys::CustomScanState,
    _pcxt: *mut pg_sys::ParallelContext,
) -> pg_sys::Size {
    0 as pg_sys::Size
}

/// Initialize the dynamic shared memory that will be required for parallel operation. coordinate
/// points to a shared memory area of size equal to the return value of EstimateDSMCustomScan. This
/// callback is optional, and need only be supplied if this custom scan provider supports parallel
/// execution.
#[pg_guard]
unsafe extern "C" fn InitializeDSMCustomScan(
    _node: *mut pg_sys::CustomScanState,
    _pcxt: *mut pg_sys::ParallelContext,
    _coordinate: *mut std::os::raw::c_void,
) {
}

/// Re-initialize the dynamic shared memory required for parallel operation when the custom-scan
/// plan node is about to be re-scanned. This callback is optional, and need only be supplied if
/// this custom scan provider supports parallel execution. Recommended practice is that this callback
/// reset only shared state, while the `ReScanCustomScan` callback resets only local state. Currently,
/// this callback will be called before `ReScanCustomScan`, but it's best not to rely on that ordering.
#[pg_guard]
unsafe extern "C" fn ReInitializeDSMCustomScan(
    _node: *mut pg_sys::CustomScanState,
    _pcxt: *mut pg_sys::ParallelContext,
    _coordinate: *mut std::os::raw::c_void,
) {
}

/// nitialize a parallel worker's local state based on the shared state set up by the leader during
/// `InitializeDSMCustomScan`. This callback is optional, and need only be supplied if this custom
/// scan provider supports parallel execution.
#[pg_guard]
unsafe extern "C" fn InitializeWorkerCustomScan(
    _node: *mut pg_sys::CustomScanState,
    _toc: *mut pg_sys::shm_toc,
    _coordinate: *mut std::os::raw::c_void,
) {
}

/// Release resources when it is anticipated the node will not be executed to completion. This is not
/// called in all cases; sometimes, `EndCustomScan` may be called without this function having been
/// called first. Since the DSM segment used by parallel query is destroyed just after this callback
/// is invoked, custom scan providers that wish to take some action before the DSM segment goes away
/// should implement this method.
#[pg_guard]
unsafe extern "C" fn ShutdownCustomScan(_node: *mut pg_sys::CustomScanState) {}

/// Output additional information for EXPLAIN of a custom-scan plan node. This callback is optional.
/// Common data stored in the `ScanState`, such as the target list and scan relation, will be shown
/// even without this callback, but the callback allows the display of additional, private state.
#[pg_guard]
unsafe extern "C" fn ExplainCustomScan(
    _node: *mut pg_sys::CustomScanState,
    _ancestors: *mut pg_sys::List,
    _es: *mut pg_sys::ExplainState,
) {
}
