mod batch;
mod bounds;
mod commands;
pub mod component_defs;
mod ctx;
mod envelope;
mod error;
mod history;
pub mod links;
mod session;
pub mod texture;

pub use bounds::{bounds_json, extent, object_bounds, scene_bounds};
pub use commands::check::{IssueCode, IssueSeverity};
pub use commands::clipboard::{Copied, copy_text};
pub use commands::results;
pub use commands::scene::ray_hits;
pub use commands::screenshot::{CameraArg, ColorMode, Mode, Overlay, ScreenshotArgs, Simulate};
pub use commands::session_args::{Attach, ClipboardCopy, RecoveryDiscard, RecoveryRestore};
pub use commands::util::descendants;
pub use commands::view::{CameraParams, build_camera, camera_json, resolve_camera};
pub use commands::{Exec, NoArgs, OPS, OpInfo, Safety, parse_args, required_safety};
pub use envelope::{Changes, Request, Response, Success};
pub use error::{CommandError, ErrorCode, ErrorResponse};

/// Typed argument structs of the ops, for clients that send requests in-process.
pub mod args {
    pub use crate::commands::NoArgs;
    pub use crate::commands::brush::Axis;
    pub use crate::commands::brush::{Clip, CreateBox, Keep};
    pub use crate::commands::defs::{Add as AddDefs, Embed as EmbedDefs, Remove as RemoveDefs};
    pub use crate::commands::entity::{
        AddComponent, Create as CreateEntity, CreateBrushEntity, Links, Members, RemoveComponent,
        SetComponent, SetTransform, UpdateComponent,
    };
    pub use crate::commands::face::{BrushVertexAt, SetMaterial, SetUv, TranslateVertices, UvMode};
    pub use crate::commands::group::{Close as CloseGroup, Open as OpenGroup};
    pub use crate::commands::layer::SetOmitFromExport;
    pub use crate::commands::material::{
        Create, Delete as DeleteMaterial, Embed, Rename as RenameMaterial, Set, Store, TextureInput,
    };
    pub use crate::commands::object::{Delete, Rename, SetLayer};
    pub use crate::commands::session_args::{Attach, Save, SelectFaces, SelectIds, SelectKind};
    pub use crate::commands::surface::{
        AlignAction, AlignUv, Anchor, FitAxes, ResetTo, TransferMode, TransferUv, UvAxis,
    };
    pub use crate::commands::transform::{
        AxisArg, Factors, Flip, Pivot, PivotName, Rotate, Scale, Translate,
    };
    pub use crate::commands::visibility::{Isolate, SetHidden, SetLocked};
}
pub use session::{Actor, Feed, FileState, HistoryState, Session, TxnInfo};
