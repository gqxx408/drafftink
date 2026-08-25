//! drafftink-migrator：ENBX → drftx 迁移核心。
//!
//! 模块划分：
//! - [`enbx_model`]：ENBX 已解析输入模型。
//! - [`whiteboard`]：drftx 输出模型（`WhiteboardDoc` / `WbShape` / `WbShapeType` 等）。
//! - [`enbx_to_wb`]：转换逻辑（`convert` / `convert_element` / `convert_shape` / `generate_report`）。
//! - [`wb_to_drftx`]：drftx 序列化（SVG Path 优先）。

pub mod enbx_model;
pub mod enbx_to_wb;
pub mod wb_to_drftx;
pub mod whiteboard;

pub use enbx_to_wb::{
    convert, convert_3d_shape, convert_activity, convert_activity_item, convert_element,
    convert_image, convert_picture, convert_shape, convert_text, convert_topic, convert_video,
    extract_text_from_topic_title, from_enbx, generate_report, parse_argb_color,
    parse_argb_color_bg, parse_location,
};
pub use wb_to_drftx::{shape_path, to_drftx};

pub use enbx_model::{
    BoardXml, Enbx3dShape, EnbxActivity, EnbxActivityItem, EnbxClassify, EnbxClassifyItem,
    EnbxElement, EnbxParsed, EnbxPicture, EnbxTopic, EnbxTopicNode, EnbxVideo, ImageXml,
    MediaReference, MigratorError, Reference, ShapeXml, SlideXml, TextRun, TextXml,
};
pub use whiteboard::{
    Canvas, MediaAsset, Metadata, MigrationNote, MigrationReport, WbElement, WbImage, WbPage,
    WbPlaceholder, WbShape, WbShapeType, WbText, WhiteboardDoc,
};
