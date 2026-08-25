//! # drafftink-core
//!
//! Core architecture for the drafftink whiteboard engine — a modern,
//! cross-platform replacement for Seewo's EasiNote.Extension.
//!
//! ## Module Layout
//!
//! | Module       | Responsibility                                    |
//! |--------------|---------------------------------------------------|
//! | `element`    | `Element` trait, `SaveInfo` trait, `ElementData`  |
//! | `command`    | `BoardCommand`, `CommandQueue`, `UndoRedoStack`   |
//! | `context`    | `BoardContext`, `Slide`, `BoardMode` (Edit/Disp)  |
//! | `registry`   | `Plugin` trait, `PluginRegistry`, `register_plugin!` |
//! | `camera`     | 2D camera (world ↔ screen transforms)             |
//! | `animation`  | Animation data model (easing, effects, sequences) |
//! | `model`      | Legacy element types (`Element` enum, `BaseElement`) |
//! | `board`      | Legacy dual-board (EditBoard / DisplayBoard)      |
//! | `history`    | Legacy undo/redo (`Command` / `History`)          |
//! | `plugin`     | Legacy dynamic plugin loading (cdylib, FFI)       |
//! | `document`   | Binary `.drft` file format I/O                    |
//! | `formats`    | File-format registry (import dispatch)            |
//! | `edit`       | Legacy edit-mode state (selection, drag, resize)  |
//! | `geometry`   | Dynamic geometry constraint solver (Kahn topo)   |
//! | `cache`      | Annotation ink-stroke cache (atomic, CRC-verified)|
//! | `drftx`      | `.drftx` 防篡改作业格式（快照/签名/批注三层）      |
//! | `crypto`     | 加密逻辑（Ed25519、JWT、设备指纹、HMAC-SHA256）   |
//! | `models`     | 公共业务数据结构（作业/用户/班级/审计日志）        |
//! | `utils`      | 工具函数（哈希、时间、Base64、文件操作）           |
//!
//! ## Architecture Principles
//!
//! 1. **No global state.** `BoardContext` is a plain struct — no `static mut`,
//!    no `lazy_static`, no `thread_local`.
//! 2. **Command pattern.** Every mutation goes through `BoardCommand`.
//! 3. **Trait-based dispatch.** Core logic uses `Element` trait, not
//!    `ElementData` match arms.
//! 4. **Plugin registration via macro.** `register_plugin!` replaces
//!    C#'s `[StartupTask]` attribute.
//! 5. **Dual-mode.** `BoardMode::Edit` / `BoardMode::Display` controls
//!    UI rendering without duplicating state.

// ── New architecture modules ─────────────────────────────────────────────

pub mod command;
pub mod context;
pub mod element;
pub mod registry;

// ── Legacy modules (preserved for backward compatibility) ────────────────

pub mod animation;
pub mod board;
pub mod cache;
pub mod camera;
pub mod document;
pub mod edit;
pub mod formats;
pub mod geometry;
pub mod history;
pub mod model;
pub mod plugin;

// ── 校本教学套件核心模块 ─────────────────────────────────────────────────

pub mod auth;
pub mod crypto;
pub mod drftx;
pub mod emgi;
/// 备授一体上层整合共享上下文（不触碰核心逻辑）。
pub mod integration;
pub mod models;
pub mod recording;
pub mod sm4;
pub mod utils;
pub mod zxx;

// ── New API re-exports (root level) ──────────────────────────────────────

pub use command::{BoardCommand, CommandQueue, UndoRedoStack};
pub use context::{BoardContext, BoardMode, DisplayState, EditState, Slide};
pub use element::ElementData;
pub use registry::{Plugin, PluginRegistry};

// ── Legacy re-exports (preserved for backward compatibility) ─────────────

pub use animation::{
    apply_easing, AnimationCategory, AnimationTrigger, Direction, Easing, EffectType,
    ElementAnimation, SlideAnimationSequence, SLIDE_BACKGROUND_ID,
};
pub use camera::Camera;
pub use history::{Command, History, MAX_DEPTH as HISTORY_MAX_DEPTH};
pub use model::{
    BaseElement, CoursewareDoc, Element, ElementId, ImageElement, PageContent, PathElement,
    ShapeElement, ShapeType, TextElement,
};

// ── 校本教学套件 re-exports ──────────────────────────────────────────────

pub use crypto::{generate_jwt, verify_jwt, verify_jwt_unchecked, JwtClaims, JwtConfig};
pub use drftx::{sign_snapshot, DrftxFile, ExerciseSignature, ExerciseSnapshot, TeacherAnnotation};
pub use emgi::{
    AwardInfo, CampusBasic, ClassInfo, CommContact, EmgiDataset, EmgiRecordable, Equipment,
    GeneralTeaching, GeneralTime, OrgBasic, PersonBasic, PunishmentInfo, SchoolBasic, Schoolhouse,
    ScoreInfo, StaffBasic, StaffEducation, StaffPartyPost, StaffTitle, StudentBasic, StudentStatus,
};
pub use integration::{SharedAppContext, SharedContext};
pub use models::{
    AuditAction, AuditLog, Class, Homework, HomeworkStatus, HomeworkSubmission, Role, School,
    SubmissionStatus, User,
};
pub use sm4::Sm4;
pub use utils::date_time::{GbDateTime, GbDateTimeError};
pub use utils::gb_industry_codes::*;
pub use utils::gb_language_codes::*;
pub use utils::gb_standard_codes::*;
pub use utils::{format_datetime, format_file_size, sha256, sha256_hex};

// ── JY/T 1004 普通中小学校管理信息 (ZXX) re-exports ──────────────
pub use zxx::xml::to_xml as zxx_to_xml;
pub use zxx::zxbg::Schedule as OfficeSchedule;
pub use zxx::zxbg::{Announcement, OfficialDoc, MANDATORY_COUNT};
pub use zxx::zxdy::{Attention, Deed};
pub use zxx::zxjx::{Course, Schedule, TeachPlan, Textbook};
pub use zxx::zxjz::{StaffPost, StaffProfile, StaffQualification};
pub use zxx::zxtw::{Medical, Sport};
pub use zxx::zxxs::{Evaluation, ExamRecord, Graduation, StudentProfile, StudentStatusProfile};
pub use zxx::zxxx::{ClassProfile, Grade, Organization, SchoolProfile, SchoolStandard};
pub use zxx::{ZxxDataset, SUBSET_NAMES};

// ── DB34/T 2318-2015 课堂录播与资源发布 re-exports ──────────────
pub use recording::{
    ActivityDirector, ContainerFormat, CoursewareClassification, CoursewareResource,
    DirectingSignals, DirectingStrategy, InteractionSummary, LiveView, RecordingMetadata,
    RecordingMode, RecordingParams, Resolution, ResourcePermission, StructuredRecording,
    VideoEncoding, DB34_STANDARD, LIVE_LATENCY_BUDGET_MS,
};
pub use zxx::integration::{
    course_from_lesson, dataset_from_components, exam_record_from_annotation,
    student_profile_from_snapshot,
};

// ── Prelude ──────────────────────────────────────────────────────────────

/// Convenience re-exports for the new architecture.
///
/// ```ignore
/// use drafftink_core::prelude::*;
///
/// let mut ctx = BoardContext::new();
/// ctx.add_element(ElementData::formula(BaseElement::default(), "sin(x)"));
/// ctx.process_commands();
/// ```
pub mod prelude {
    pub use crate::command::{BoardCommand, CommandQueue, UndoRedoStack};
    pub use crate::context::{BoardContext, BoardMode, DisplayState, EditState, Slide};
    pub use crate::element::{Element, ElementData, ElementId, SaveInfo};
    pub use crate::registry::{Plugin, PluginRegistry};
}
