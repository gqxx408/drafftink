//! # 国标（GB/T）代码表查询接口（方向一：数据序列化与 API 对接）
//!
//! 提供统一的只读查询入口，使前端 / 移动端 / 第三方系统可通过 HTTP 读取
//! `drafftink-core` 中硬编码的国标代码表。
//!
//! ## 路由
//!
//! `GET /api/v1/lookup/{table}?code=xxx`
//!
//! 支持的 `{table}`：
//! - `school_type`   办学类型（GB/T 33782-2017）
//! - `province`      省级行政区（GB/T 2260-2007）
//! - `urban_rural`   城乡类型（GB/T 33782-2017）
//! - `gender`        性别（GB/T 2261.1-2003）
//! - `yesno`         是否标志（系统内部约定）
//! - `education_level` 学历（GB/T 4658-2006）
//! - `degree`        学位（GB/T 6864-2003）
//! - `tech_position` 专业技术职务（GB/T 8561-2001）
//! - `ethnic`        民族（GB/T 3304-1991）
//! - `marital_status` 婚姻状况（GB/T 2261.2-2003）
//! - `industry_section` / `industry_division` / `industry_class` 行业分类（GB/T 4754-2017）
//! - `language`      语种（GB/T 4881-1985，已冻结待校核）
//!
//! 返回示例：`GET /api/v1/lookup/school_type?code=211`
//! ```json
//! { "table": "school_type", "standard": "GB/T 33782-2017", "code": "211", "name": "小学", "found": true }
//! ```

use axum::extract::{Path, Query};
use axum::Json;
use serde::Deserialize;
use serde::Serialize;

use drafftink_core::utils::gb_industry_codes::{
    get_industry_class_name, get_industry_division_name, get_industry_section_name,
};
use drafftink_core::utils::gb_language_codes::get_language_name;
use drafftink_core::utils::gb_standard_codes::{
    get_degree_name, get_education_level_name, get_ethnic_name, get_marital_status_name,
    get_province_name, get_school_type_name, get_tech_position_name, get_urban_rural_name,
    GenderCode, YesNoCode,
};

/// 查询参数：`?code=xxx`。
#[derive(Deserialize)]
pub struct LookupQuery {
    pub code: String,
}

/// 统一的查询响应结构。
#[derive(Serialize)]
pub struct LookupResponse {
    /// 查询的表名（即路由 `{table}`）。
    pub table: String,
    /// 对应的 GB/T 标准号；未知表为空串。
    pub standard: String,
    /// 原始查询代码（原样回显）。
    pub code: String,
    /// 命中后的中文名称；未命中为 `None`。
    pub name: Option<String>,
    /// 是否命中有效代码。
    pub found: bool,
}

/// 构造一条响应；`name` 为 `None` 时 `found` 自动为 `false`。
fn respond(table: &str, standard: &str, code: &str, name: Option<String>) -> LookupResponse {
    LookupResponse {
        table: table.to_string(),
        standard: standard.to_string(),
        code: code.to_string(),
        found: name.is_some(),
        name,
    }
}

/// 纯查询逻辑（无 I/O、可单测）：根据表名与代码返回命中名称。
///
/// 所有数值型代码先做 `parse`，解析失败或代码未命中均归为 `found = false`。
pub(crate) fn lookup_code(table: &str, code: &str) -> LookupResponse {
    match table {
        // ── 数值码表 ──
        "school_type" => {
            let name = code
                .parse::<u16>()
                .ok()
                .and_then(|c| get_school_type_name(c).map(str::to_string));
            respond("school_type", "GB/T 33782-2017", code, name)
        }
        "province" => {
            let name = code
                .parse::<u32>()
                .ok()
                .and_then(|c| get_province_name(c).map(str::to_string));
            respond("province", "GB/T 2260-2007", code, name)
        }
        "urban_rural" => {
            let name = code
                .parse::<u16>()
                .ok()
                .and_then(|c| get_urban_rural_name(c).map(str::to_string));
            respond("urban_rural", "GB/T 33782-2017", code, name)
        }
        "gender" => {
            let name = code
                .parse::<u8>()
                .ok()
                .and_then(|c| GenderCode::from_code(c).map(|g| g.name().to_string()));
            respond("gender", "GB/T 2261.1-2003", code, name)
        }
        "yesno" => {
            let name = code
                .parse::<u8>()
                .ok()
                .and_then(|c| YesNoCode::from_code(c).map(|y| y.name().to_string()));
            respond("yesno", "INTERNAL", code, name)
        }
        // ── 字符串码表 ──
        "education_level" => respond(
            "education_level",
            "GB/T 4658-2006",
            code,
            get_education_level_name(code).map(str::to_string),
        ),
        "degree" => respond(
            "degree",
            "GB/T 6864-2003",
            code,
            get_degree_name(code).map(str::to_string),
        ),
        "tech_position" => respond(
            "tech_position",
            "GB/T 8561-2001",
            code,
            get_tech_position_name(code).map(str::to_string),
        ),
        "ethnic" => respond(
            "ethnic",
            "GB/T 3304-1991",
            code,
            get_ethnic_name(code).map(str::to_string),
        ),
        "marital_status" => respond(
            "marital_status",
            "GB/T 2261.2-2003",
            code,
            get_marital_status_name(code).map(str::to_string),
        ),
        // ── 行业分类（GB/T 4754-2017，机器提取） ──
        "industry_section" => respond(
            "industry_section",
            "GB/T 4754-2017",
            code,
            get_industry_section_name(code).map(str::to_string),
        ),
        "industry_division" => respond(
            "industry_division",
            "GB/T 4754-2017",
            code,
            get_industry_division_name(code).map(str::to_string),
        ),
        "industry_class" => respond(
            "industry_class",
            "GB/T 4754-2017",
            code,
            get_industry_class_name(code).map(str::to_string),
        ),
        // ── 语种（GB/T 4881-1985，已冻结待校核） ──
        "language" => respond(
            "language",
            "GB/T 4881-1985",
            code,
            get_language_name(code).map(str::to_string),
        ),
        // ── 未知表 ──
        _ => LookupResponse {
            table: table.to_string(),
            standard: String::new(),
            code: code.to_string(),
            name: None,
            found: false,
        },
    }
}

/// GET /api/v1/lookup/{table}?code=xxx
///
/// 只读国标代码查询，无需认证（与 `/api/health` 同属公开路由）。
pub async fn lookup(
    Path(table): Path<String>,
    Query(q): Query<LookupQuery>,
) -> Json<LookupResponse> {
    Json(lookup_code(&table, &q.code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_school_type_211() {
        let r = lookup_code("school_type", "211");
        assert!(r.found);
        assert_eq!(r.name.as_deref(), Some("小学"));
        assert_eq!(r.standard, "GB/T 33782-2017");
    }

    #[test]
    fn test_lookup_province_440000() {
        let r = lookup_code("province", "440000");
        assert!(r.found);
        assert_eq!(r.name.as_deref(), Some("广东省"));
    }

    #[test]
    fn test_lookup_gender() {
        assert_eq!(lookup_code("gender", "1").name.as_deref(), Some("男"));
        assert_eq!(lookup_code("gender", "2").name.as_deref(), Some("女"));
        assert!(!lookup_code("gender", "3").found);
    }

    #[test]
    fn test_lookup_yesno() {
        assert_eq!(lookup_code("yesno", "1").name.as_deref(), Some("是"));
        assert_eq!(lookup_code("yesno", "0").name.as_deref(), Some("否"));
    }

    #[test]
    fn test_lookup_unknown_table_and_code() {
        assert!(!lookup_code("school_type", "999").found);
        assert!(!lookup_code("nope", "211").found);
        assert_eq!(lookup_code("nope", "211").standard, "");
    }

    #[test]
    fn test_lookup_industry_class() {
        // GB/T 4754-2017 机器提取样本：0111 = 稻谷种植
        let r = lookup_code("industry_class", "0111");
        assert!(r.found);
        assert_eq!(r.name.as_deref(), Some("稻谷种植"));
    }

    #[test]
    fn test_lookup_language_frozen() {
        // GB/T 4881-1985：1 = 汉语（已冻结，仅供存量兼容）
        let r = lookup_code("language", "1");
        assert!(r.found);
        assert_eq!(r.name.as_deref(), Some("汉语"));
    }
}
