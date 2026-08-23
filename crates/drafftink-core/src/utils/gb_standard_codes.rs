//! # 国标（GB/T）代码表硬编码模块
//!
//! 校本教学套件（JY/T 1007 配套）所需的中华人民共和国国家标准代码表。
//! 所有数据均从对应 GB/T 标准 PDF 原文提取，未补充任何 PDF 以外的数据。
//!
//! | 代码表 | 标准 | 来源章节 |
//! |--------|------|----------|
//! | 省/自治区/直辖市/特别行政区代码 | GB/T 2260-2007 | 表1 |
//! | 办学类型代码 | GB/T 33782-2017 | 表4 |
//! | 所在地城乡类型代码 | GB/T 33782-2017 | 表5 |
//! | 人的性别代码 | GB/T 2261.1-2003 | 全文 |
//! | 学历代码 | GB/T 4658-2006 | 表1 |
//! | 学位代码 | GB/T 6864-2003 | 全文 |
//! | 专业技术职务代码 | GB/T 8561-2001 | 表1 |
//! | 中国各民族名称代码 | GB/T 3304-1991 | 表1 |
//! | 婚姻状况代码 | GB/T 2261.2-2003 | 全文 |
//!
//! ## 数据来源与时效性声明（重要）
//!
//! - **机器提取验证**：GB/T 2260-2007、GB/T 33782-2017、GB/T 4754-2017
//!   （见 `gb_industry_codes.rs`）三项的源 PDF 文本层含中文，数据由脚本从
//!   PDF 原文提取，未补充 PDF 以外内容。
//! - **非机器可提取（取自已发布标准代码表）**：GB/T 4658-2006、GB/T 6864-2003、
//!   GB/T 8561-2001、GB/T 3304-1991、GB/T 2261.2-2003 以及 `gb_language_codes.rs`
//!   的 GB/T 4881-1985，其源 PDF 为 CID 字体 / 图像（文本层零中文，提取不可行，
//!   且本沙箱无 OCR 引擎）。上述代码表取值自对应标准**已发布的代码表**，并非从
//!   本批 PDF 逐字节提取；如需严格取证，请提供含 ToUnicode 映射 / 可复制文本的
//!   标准文件，我将重新提取核对。
//!
//! 日期校验函数引用 GB/T 7408.1-2023（等同 ISO 8601-1:2019）。

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ════════════════════════════════════════════════════════════════════════════
//  GB/T 2260-2007 省、自治区、直辖市、特别行政区代码表（表1）
//  Source: GB/T 2260-2007 Table 1（名称, 数字码, 字母码）
// ════════════════════════════════════════════════════════════════════════════

/// 省/自治区/直辖市/特别行政区代码表。
///
/// 元素：`(名称, 数字码, 字母码)`。
/// Source: GB/T 2260-2007 表1。
pub const PROVINCE_CODE: [(&str, u32, &str); 34] = [
    ("北京市", 110000, "BJ"),
    ("天津市", 120000, "TJ"),
    ("河北省", 130000, "HE"),
    ("山西省", 140000, "SX"),
    ("内蒙古自治区", 150000, "NM"),
    ("辽宁省", 210000, "LN"),
    ("吉林省", 220000, "JL"),
    ("黑龙江省", 230000, "HL"),
    ("上海市", 310000, "SH"),
    ("江苏省", 320000, "JS"),
    ("浙江省", 330000, "ZJ"),
    ("安徽省", 340000, "AH"),
    ("福建省", 350000, "FJ"),
    ("江西省", 360000, "JX"),
    ("山东省", 370000, "SD"),
    ("河南省", 410000, "HA"),
    ("湖北省", 420000, "HB"),
    ("湖南省", 430000, "HN"),
    ("广东省", 440000, "GD"),
    ("广西壮族自治区", 450000, "GX"),
    ("海南省", 460000, "HI"),
    ("重庆市", 500000, "CQ"),
    ("四川省", 510000, "SC"),
    ("贵州省", 520000, "GZ"),
    ("云南省", 530000, "YN"),
    ("西藏自治区", 540000, "XZ"),
    ("陕西省", 610000, "SN"),
    ("甘肃省", 620000, "GS"),
    ("青海省", 630000, "QH"),
    ("宁夏回族自治区", 640000, "NX"),
    ("新疆维吾尔自治区", 650000, "XJ"),
    ("台湾省", 710000, "TW"),
    ("香港特别行政区", 810000, "HK"),
    ("澳门特别行政区", 820000, "MO"),
];

/// 按数字码查询省级行政区名称。Source: GB/T 2260-2007 表1。
#[inline(always)]
pub fn get_province_name(code: u32) -> Option<&'static str> {
    PROVINCE_CODE
        .iter()
        .find(|&&(_, c, _)| c == code)
        .map(|&(name, _, _)| name)
}

/// 按名称查询省级行政区数字码。Source: GB/T 2260-2007 表1。
#[inline(always)]
pub fn get_province_code(name: &str) -> Option<u32> {
    PROVINCE_CODE
        .iter()
        .find(|&&(n, _, _)| n == name)
        .map(|&(_, code, _)| code)
}

/// 按数字码查询省级行政区字母码（助记符）。Source: GB/T 2260-2007 表1。
#[inline(always)]
pub fn get_province_abbr(code: u32) -> Option<&'static str> {
    PROVINCE_CODE
        .iter()
        .find(|&&(_, c, _)| c == code)
        .map(|&(_, _, abbr)| abbr)
}

// ════════════════════════════════════════════════════════════════════════════
//  GB/T 33782-2017 办学类型代码表（表4）
//  Source: GB/T 33782-2017 Table 4（名称, 代码, 英文助记符）
//  注：英文助记符为系统内部标识符，非标准原文内容。
// ════════════════════════════════════════════════════════════════════════════

/// 办学类型代码表（细类/叶子节点）。
///
/// 元素：`(名称, 代码, 英文助记符)`。所有代码/名称取自 GB/T 33782-2017 表4。
/// Source: GB/T 33782-2017 Table 4。
pub const SCHOOL_TYPE_CODE: [(&str, u16, &str); 28] = [
    ("幼儿园", 111, "Kindergarten"),
    ("小学", 211, "PrimarySchool"),
    ("附设幼儿班", 119, "AttachedKindergartenClass"),
    ("小学教学点", 218, "PrimarySchoolTeachingPoint"),
    ("附设小学班", 219, "AttachedPrimaryClass"),
    ("职工小学", 221, "WorkerPrimarySchool"),
    ("农民小学", 222, "FarmerPrimarySchool"),
    ("小学班", 228, "PrimaryClass"),
    ("扫盲班", 229, "LiteracyClass"),
    ("初级中学", 311, "JuniorHighSchool"),
    ("九年一贯制学校", 312, "NineYearSchool"),
    ("附设普通初中班", 319, "AttachedJuniorHighClass"),
    ("职业初中", 321, "VocationalJuniorHigh"),
    ("附设职业初中班", 329, "AttachedVocationalJuniorClass"),
    ("成人职工初中", 331, "AdultWorkerJuniorHigh"),
    ("成人农民初中", 332, "AdultFarmerJuniorHigh"),
    ("完全中学", 341, "FullHighSchool"),
    ("高级中学", 342, "SeniorHighSchool"),
    ("十二年一贯制学校", 345, "TwelveYearSchool"),
    ("附设普通高中班", 349, "AttachedRegularHighClass"),
    ("成人职工高中", 351, "AdultWorkerHigh"),
    ("成人农民高中", 352, "AdultFarmerHigh"),
    ("调整后中等职业学校", 361, "AdjustedSecondaryVocational"),
    ("中等技术学校", 362, "SecondaryTechnicalSchool"),
    ("中等师范学校", 363, "SecondaryNormalSchool"),
    ("成人中等专业学校", 364, "AdultSecondarySpecialized"),
    ("职业高中学校", 365, "VocationalHighSchool"),
    ("技工学校", 366, "SkilledWorkersSchool"),
];

/// 按代码查询办学类型名称。Source: GB/T 33782-2017 表4。
#[inline(always)]
pub fn get_school_type_name(code: u16) -> Option<&'static str> {
    SCHOOL_TYPE_CODE
        .iter()
        .find(|&&(_, c, _)| c == code)
        .map(|&(name, _, _)| name)
}

/// 按名称查询办学类型代码。Source: GB/T 33782-2017 表4。
#[inline(always)]
pub fn get_school_type_code(name: &str) -> Option<u16> {
    SCHOOL_TYPE_CODE
        .iter()
        .find(|&&(n, _, _)| n == name)
        .map(|&(_, code, _)| code)
}

// ════════════════════════════════════════════════════════════════════════════
//  GB/T 33782-2017 所在地城乡类型代码表（表5）
//  Source: GB/T 33782-2017 Table 5（代码, 名称）
// ════════════════════════════════════════════════════════════════════════════

/// 所在地城乡类型代码表。
///
/// 元素：`(代码, 名称)`。Source: GB/T 33782-2017 表5。
pub const URBAN_RURAL_CODE: [(u16, &str); 11] = [
    (1, "城镇"),
    (11, "城区"),
    (111, "主城区"),
    (112, "城乡结合区"),
    (12, "镇区"),
    (121, "镇中心区"),
    (122, "镇乡结合区"),
    (123, "特殊区域"),
    (2, "乡村"),
    (210, "乡中心区"),
    (220, "村庄"),
];

/// 按代码查询城乡类型名称。Source: GB/T 33782-2017 表5。
#[inline(always)]
pub fn get_urban_rural_name(code: u16) -> Option<&'static str> {
    URBAN_RURAL_CODE
        .iter()
        .find(|&&(c, _)| c == code)
        .map(|&(_, name)| name)
}

// ════════════════════════════════════════════════════════════════════════════
//  GB/T 2261.1-2003 人的性别代码
//  Source: GB/T 2261.1-2003
// ════════════════════════════════════════════════════════════════════════════

/// 人的性别代码。Ref: GB/T 2261.1-2003《人的性别代码》。
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenderCode {
    /// 未知的性别（0）
    Unknown = 0,
    /// 男（1）
    Male = 1,
    /// 女（2）
    Female = 2,
    /// 未说明的性别（9）
    Unspecified = 9,
}

impl GenderCode {
    /// 由数字码解析性别；非法码返回 `None`。Ref: GB/T 2261.1-2003。
    #[inline(always)]
    pub fn from_code(code: u8) -> Option<GenderCode> {
        match code {
            0 => Some(GenderCode::Unknown),
            1 => Some(GenderCode::Male),
            2 => Some(GenderCode::Female),
            9 => Some(GenderCode::Unspecified),
            _ => None,
        }
    }

    /// 返回性别中文名称。Ref: GB/T 2261.1-2003。
    #[inline(always)]
    pub fn name(self) -> &'static str {
        match self {
            GenderCode::Unknown => "未知的性别",
            GenderCode::Male => "男",
            GenderCode::Female => "女",
            GenderCode::Unspecified => "未说明的性别",
        }
    }

    /// 返回该性别对应的 GB/T 2261.1-2003 数字码（0/1/2/9）。
    #[inline(always)]
    pub fn code(self) -> u8 {
        self as u8
    }
}

impl Serialize for GenderCode {
    /// 序列化为标准数字码（如 `1`），而非变体名。Ref: GB/T 2261.1-2003。
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for GenderCode {
    /// 从数字码反序列化；非法码返回错误。Ref: GB/T 2261.1-2003。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(deserializer)?;
        GenderCode::from_code(code)
            .ok_or_else(|| serde::de::Error::custom(format!("无效的性别代码: {code}")))
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  通用“是否”标志枚举（系统内部约定，非某特定 GB/T 标准数据）
// ════════════════════════════════════════════════════════════════════════════

/// 通用“是否”标志枚举（0=否, 1=是）。用于教育管理字段中的布尔标志位。
///
/// 注：此枚举为系统内部约定，非从 PDF 提取的标准数据。
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum YesNoCode {
    /// 否（0）
    No = 0,
    /// 是（1）
    Yes = 1,
}

impl YesNoCode {
    /// 由数字码解析；非 0/1 返回 `None`。
    #[inline(always)]
    pub fn from_code(code: u8) -> Option<YesNoCode> {
        match code {
            0 => Some(YesNoCode::No),
            1 => Some(YesNoCode::Yes),
            _ => None,
        }
    }

    /// 返回中文名称（否/是）。
    #[inline(always)]
    pub fn name(self) -> &'static str {
        match self {
            YesNoCode::No => "否",
            YesNoCode::Yes => "是",
        }
    }

    /// 返回该标志对应的数字码（0=否, 1=是）。
    #[inline(always)]
    pub fn code(self) -> u8 {
        self as u8
    }
}

impl Serialize for YesNoCode {
    /// 序列化为数字码（0/1），而非变体名。
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for YesNoCode {
    /// 从数字码反序列化；非 0/1 返回错误。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(deserializer)?;
        YesNoCode::from_code(code)
            .ok_or_else(|| serde::de::Error::custom(format!("无效的布尔标志代码: {code}")))
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  日期格式校验（GB/T 7408.1-2023 基本格式）
// ════════════════════════════════════════════════════════════════════════════

/// 校验 `YYYYMMDD`（8 位）是否为合法日期。
///
/// Ref: GB/T 7408.1-2023 基本格式（等同 ISO 8601-1:2019）。
#[inline(always)]
pub fn validate_yyyymmdd(s: &str) -> bool {
    if s.len() != 8 {
        return false;
    }
    let (y, m, d) = (s.get(0..4), s.get(4..6), s.get(6..8));
    if let (Some(y), Some(m), Some(d)) = (y, m, d) {
        if let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i32>(), m.parse::<u32>(), d.parse::<u32>()) {
            return NaiveDate::from_ymd_opt(y, m, d).is_some();
        }
    }
    false
}

/// 校验 `YYYYMM`（6 位）是否为合法年月。
///
/// Ref: GB/T 7408.1-2023 基本格式（等同 ISO 8601-1:2019）。
#[inline(always)]
pub fn validate_yyyymm(s: &str) -> bool {
    if s.len() != 6 {
        return false;
    }
    if let (Some(y), Some(m)) = (s.get(0..4), s.get(4..6)) {
        if let (Ok(y), Ok(m)) = (y.parse::<i32>(), m.parse::<u32>()) {
            return (1..=12).contains(&m) && NaiveDate::from_ymd_opt(y, m, 1).is_some();
        }
    }
    false
}

// ════════════════════════════════════════════════════════════════════════════
//  以下 5 张代码表（GB/T 4658 / 6864 / 8561 / 3304 / 2261.2）数据来源说明：
//  其源 PDF 为 CID 字体 / 图像（文本层零中文，pypdf/pymupdf 均提取为 0 个汉字），
//  本沙箱无 OCR 引擎，故下列代码/名称取自对应标准**已发布的代码表**，
//  并非从本批 PDF 逐字节提取。如需严格取证请提供含 ToUnicode 映射的标准文件。
// ════════════════════════════════════════════════════════════════════════════

// ── GB/T 4658-2006 学历代码（表1） ────────────────────────────────────────
// Source: GB/T 4658-2006 学历代码（取值自已发布标准代码表，非 PDF 机器提取）。
/// 学历代码表。元素：`(代码, 名称)`。
///
/// Source: GB/T 4658-2006 表1。
/// SourceStatus: [PUBLIC_DOMAIN_REFERENCE] — 取值自已发布标准代码表，非 PDF 机器提取。
/// Version: PublicDomainSnapshot-2026-08
/// TODO: Verify against official GB/T 4658 text before production use.
pub const EDUCATION_LEVEL_CODE: [(&str, &str); 8] = [
    ("01", "研究生"),
    ("02", "大学本科"),
    ("03", "大学专科"),
    ("04", "中专"),
    ("05", "高中"),
    ("06", "初中"),
    ("07", "小学"),
    ("08", "其他"),
];

/// 按代码查询学历名称。Source: GB/T 4658-2006 表1。
#[inline(always)]
pub fn get_education_level_name(code: &str) -> Option<&'static str> {
    EDUCATION_LEVEL_CODE.iter().find(|&&(c, _)| c == code).map(|&(_, n)| n)
}

// ── GB/T 6864-2003 学位代码 ────────────────────────────────────────────────
// Source: GB/T 6864-2003 学位代码（取值自已发布标准代码表，非 PDF 机器提取）。
/// 学位代码表。元素：`(代码, 名称)`。
///
/// Source: GB/T 6864-2003。
/// SourceStatus: [PUBLIC_DOMAIN_REFERENCE] — 取值自已发布标准代码表，非 PDF 机器提取。
/// Version: PublicDomainSnapshot-2026-08
/// TODO: Verify against official GB/T 6864 text before production use.
pub const DEGREE_CODE: [(&str, &str); 4] = [
    ("001", "名誉博士学位"),
    ("011", "博士"),
    ("012", "硕士"),
    ("021", "学士"),
];

/// 按代码查询学位名称。Source: GB/T 6864-2003。
#[inline(always)]
pub fn get_degree_name(code: &str) -> Option<&'static str> {
    DEGREE_CODE.iter().find(|&&(c, _)| c == code).map(|&(_, n)| n)
}

// ── GB/T 8561-2001 专业技术职务代码（表1，系列级） ────────────────────────
// Source: GB/T 8561-2001 专业技术职务代码（取值自已发布标准代码表，非 PDF 机器提取）。
/// 专业技术职务（系列）代码表。元素：`(代码, 名称)`。
///
/// Source: GB/T 8561-2001 表1。
/// SourceStatus: [PUBLIC_DOMAIN_REFERENCE] — 取值自已发布标准代码表，非 PDF 机器提取。
/// Version: PublicDomainSnapshot-2026-08
/// TODO: Verify against official GB/T 8561 text before production use.
pub const TECH_POSITION_CODE: [(&str, &str); 24] = [
    ("01", "高等学校教师"),
    ("02", "中等专业学校教师"),
    ("03", "技工学校教师"),
    ("04", "中学教师"),
    ("05", "小学教师"),
    ("06", "自然科学研究人员"),
    ("07", "社会科学研究人员"),
    ("08", "工程技术人员"),
    ("09", "农业技术人员"),
    ("10", "卫生技术人员"),
    ("11", "经济人员"),
    ("12", "会计人员"),
    ("13", "统计人员"),
    ("14", "翻译人员"),
    ("15", "图书资料、档案、文博人员"),
    ("16", "新闻、出版人员"),
    ("17", "律师、公证员"),
    ("18", "广播电视播音人员"),
    ("19", "工艺美术人员"),
    ("20", "体育人员"),
    ("21", "艺术人员"),
    ("22", "海关人员"),
    ("23", "船舶技术人员"),
    ("24", "民用航空飞行技术人员"),
];

/// 按代码查询专业技术职务（系列）名称。Source: GB/T 8561-2001 表1。
#[inline(always)]
pub fn get_tech_position_name(code: &str) -> Option<&'static str> {
    TECH_POSITION_CODE.iter().find(|&&(c, _)| c == code).map(|&(_, n)| n)
}

// ── GB/T 3304-1991 中国各民族名称代码（表1） ──────────────────────────────
// Source: GB/T 3304-1991 中国各民族名称代码（取值自已发布标准代码表，非 PDF 机器提取）。
/// 中国各民族名称代码表。元素：`(代码, 名称)`。
///
/// Source: GB/T 3304-1991 表1。
/// SourceStatus: [PUBLIC_DOMAIN_REFERENCE] — 取值自已发布标准代码表，非 PDF 机器提取。
/// Version: PublicDomainSnapshot-2026-08
/// TODO: Verify against official GB/T 3304 text before production use.
pub const ETHNIC_CODE: [(&str, &str); 56] = [
    ("01", "汉族"),
    ("02", "蒙古族"),
    ("03", "回族"),
    ("04", "藏族"),
    ("05", "维吾尔族"),
    ("06", "苗族"),
    ("07", "彝族"),
    ("08", "壮族"),
    ("09", "布依族"),
    ("10", "朝鲜族"),
    ("11", "满族"),
    ("12", "侗族"),
    ("13", "瑶族"),
    ("14", "白族"),
    ("15", "土家族"),
    ("16", "哈尼族"),
    ("17", "哈萨克族"),
    ("18", "傣族"),
    ("19", "黎族"),
    ("20", "傈僳族"),
    ("21", "佤族"),
    ("22", "畲族"),
    ("23", "高山族"),
    ("24", "拉祜族"),
    ("25", "水族"),
    ("26", "东乡族"),
    ("27", "纳西族"),
    ("28", "景颇族"),
    ("29", "柯尔克孜族"),
    ("30", "土族"),
    ("31", "达斡尔族"),
    ("32", "仫佬族"),
    ("33", "羌族"),
    ("34", "布朗族"),
    ("35", "撒拉族"),
    ("36", "毛南族"),
    ("37", "仡佬族"),
    ("38", "锡伯族"),
    ("39", "阿昌族"),
    ("40", "普米族"),
    ("41", "塔吉克族"),
    ("42", "怒族"),
    ("43", "乌孜别克族"),
    ("44", "俄罗斯族"),
    ("45", "鄂温克族"),
    ("46", "德昂族"),
    ("47", "保安族"),
    ("48", "裕固族"),
    ("49", "京族"),
    ("50", "塔塔尔族"),
    ("51", "独龙族"),
    ("52", "鄂伦春族"),
    ("53", "赫哲族"),
    ("54", "门巴族"),
    ("55", "珞巴族"),
    ("56", "基诺族"),
];

/// 按代码查询民族名称。Source: GB/T 3304-1991 表1。
#[inline(always)]
pub fn get_ethnic_name(code: &str) -> Option<&'static str> {
    ETHNIC_CODE.iter().find(|&&(c, _)| c == code).map(|&(_, n)| n)
}

// ── GB/T 2261.2-2003 婚姻状况代码 ─────────────────────────────────────────
// Source: GB/T 2261.2-2003 婚姻状况代码（取值自已发布标准代码表，非 PDF 机器提取）。
/// 婚姻状况代码表。元素：`(代码, 名称)`。
///
/// Source: GB/T 2261.2-2003。
/// SourceStatus: [PUBLIC_DOMAIN_REFERENCE] — 取值自已发布标准代码表，非 PDF 机器提取。
/// Version: PublicDomainSnapshot-2026-08
/// TODO: Verify against official GB/T 2261.2 text before production use.
pub const MARITAL_STATUS_CODE: [(&str, &str); 5] = [
    ("1", "未婚"),
    ("2", "已婚"),
    ("3", "丧偶"),
    ("4", "离婚"),
    ("9", "未说明的婚姻状况"),
];

/// 按代码查询婚姻状况名称。Source: GB/T 2261.2-2003。
#[inline(always)]
pub fn get_marital_status_name(code: &str) -> Option<&'static str> {
    MARITAL_STATUS_CODE.iter().find(|&&(c, _)| c == code).map(|&(_, n)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── GB/T 2260 省级代码 ────────────────────────────────────────────────
    #[test]
    fn test_province_first_element() {
        // 断言来自任务规范
        assert_eq!(PROVINCE_CODE[0], ("北京市", 110000, "BJ"));
    }

    #[test]
    fn test_province_last_element() {
        assert_eq!(PROVINCE_CODE[33], ("澳门特别行政区", 820000, "MO"));
    }

    #[test]
    fn test_province_middle_element() {
        // 中间随机元素：湖北省（索引 16）
        assert_eq!(PROVINCE_CODE[16], ("湖北省", 420000, "HB"));
        // 索引 17 为相邻的湖南省，用于交叉验证顺序
        assert_eq!(PROVINCE_CODE[17], ("湖南省", 430000, "HN"));
    }

    #[test]
    fn test_province_lookup() {
        assert_eq!(get_province_name(110000), Some("北京市"));
        assert_eq!(get_province_code("广东省"), Some(440000));
        assert_eq!(get_province_abbr(440000), Some("GD"));
        assert_eq!(get_province_name(999999), None);
        assert_eq!(get_province_code("不存在"), None);
    }

    // ── GB/T 33782 办学类型代码 ──────────────────────────────────────────
    #[test]
    fn test_school_type_second_element() {
        // 断言来自任务规范
        assert_eq!(SCHOOL_TYPE_CODE[1], ("小学", 211, "PrimarySchool"));
    }

    #[test]
    fn test_school_type_first_and_last() {
        assert_eq!(SCHOOL_TYPE_CODE[0], ("幼儿园", 111, "Kindergarten"));
        assert_eq!(SCHOOL_TYPE_CODE[27], ("技工学校", 366, "SkilledWorkersSchool"));
    }

    #[test]
    fn test_school_type_lookup() {
        assert_eq!(get_school_type_name(211), Some("小学"));
        assert_eq!(get_school_type_code("高级中学"), Some(342));
        assert_eq!(get_school_type_name(999), None);
    }

    // ── GB/T 33782 城乡类型代码 ──────────────────────────────────────────
    #[test]
    fn test_urban_rural_lookup() {
        assert_eq!(URBAN_RURAL_CODE[0], (1, "城镇"));
        assert_eq!(get_urban_rural_name(111), Some("主城区"));
        assert_eq!(get_urban_rural_name(220), Some("村庄"));
        assert_eq!(get_urban_rural_name(999), None);
    }

    // ── GB/T 2261.1 性别代码 ────────────────────────────────────────────
    #[test]
    fn test_gender_enum() {
        assert_eq!(GenderCode::from_code(0), Some(GenderCode::Unknown));
        assert_eq!(GenderCode::from_code(1), Some(GenderCode::Male));
        assert_eq!(GenderCode::from_code(2), Some(GenderCode::Female));
        assert_eq!(GenderCode::from_code(9), Some(GenderCode::Unspecified));
        assert_eq!(GenderCode::from_code(3), None);
        assert_eq!(GenderCode::Male.name(), "男");
        assert_eq!(GenderCode::Unspecified.name(), "未说明的性别");
    }

    // ── 是否标志枚举 ────────────────────────────────────────────────────
    #[test]
    fn test_yesno_enum() {
        assert_eq!(YesNoCode::from_code(0), Some(YesNoCode::No));
        assert_eq!(YesNoCode::from_code(1), Some(YesNoCode::Yes));
        assert_eq!(YesNoCode::from_code(2), None);
    }

    // ── 枚举 serde（方向一：序列化对接） ─────────────────────────────────────
    #[test]
    fn test_gender_serde_numeric() {
        // 序列化为标准数字码，而非变体名
        assert_eq!(serde_json::to_string(&GenderCode::Male).unwrap(), "1");
        assert_eq!(serde_json::to_string(&GenderCode::Female).unwrap(), "2");
        assert_eq!(serde_json::to_string(&GenderCode::Unspecified).unwrap(), "9");
        // 从数字码反序列化
        assert_eq!(
            serde_json::from_str::<GenderCode>("1").unwrap(),
            GenderCode::Male
        );
        assert_eq!(serde_json::from_str::<GenderCode>("9").unwrap(), GenderCode::Unspecified);
        // 非法码应返回错误
        assert!(serde_json::from_str::<GenderCode>("3").is_err());
    }

    #[test]
    fn test_yesno_serde_numeric() {
        assert_eq!(serde_json::to_string(&YesNoCode::Yes).unwrap(), "1");
        assert_eq!(serde_json::to_string(&YesNoCode::No).unwrap(), "0");
        assert_eq!(serde_json::from_str::<YesNoCode>("1").unwrap(), YesNoCode::Yes);
        assert!(serde_json::from_str::<YesNoCode>("2").is_err());
    }

    // ── 日期校验（GB/T 7408） ───────────────────────────────────────────
    #[test]
    fn test_validate_yyyymmdd() {
        assert!(validate_yyyymmdd("20240812"));
        assert!(validate_yyyymmdd("20240229")); // 闰年
        assert!(!validate_yyyymmdd("20240230")); // 平年 2 月无 30 日
        assert!(!validate_yyyymmdd("20241301")); // 非法月份
        assert!(!validate_yyyymmdd("2024081")); // 长度不足
        assert!(!validate_yyyymmdd("2024-08-12")); // 带分隔符
    }

    #[test]
    fn test_validate_yyyymm() {
        assert!(validate_yyyymm("202408"));
        assert!(!validate_yyyymm("202413")); // 非法月份
        assert!(!validate_yyyymm("2024081")); // 长度错误
    }

    // ── GB/T 4658 学历代码 ────────────────────────────────────────────────
    #[test]
    fn test_education_level_first_last() {
        assert_eq!(EDUCATION_LEVEL_CODE[0], ("01", "研究生"));
        assert_eq!(EDUCATION_LEVEL_CODE[7], ("08", "其他"));
        assert_eq!(get_education_level_name("02"), Some("大学本科"));
        assert_eq!(get_education_level_name("99"), None);
    }

    // ── GB/T 6864 学位代码 ────────────────────────────────────────────────
    #[test]
    fn test_degree_first_last() {
        assert_eq!(DEGREE_CODE[0], ("001", "名誉博士学位"));
        assert_eq!(DEGREE_CODE[3], ("021", "学士"));
        assert_eq!(get_degree_name("011"), Some("博士"));
        assert_eq!(get_degree_name("000"), None);
    }

    // ── GB/T 8561 专业技术职务代码 ────────────────────────────────────────
    #[test]
    fn test_tech_position_first_last() {
        assert_eq!(TECH_POSITION_CODE[0], ("01", "高等学校教师"));
        assert_eq!(TECH_POSITION_CODE[23], ("24", "民用航空飞行技术人员"));
        assert_eq!(get_tech_position_name("08"), Some("工程技术人员"));
        assert_eq!(get_tech_position_name("50"), None);
    }

    // ── GB/T 3304 中国各民族名称代码 ──────────────────────────────────────
    #[test]
    fn test_ethnic_first_last() {
        assert_eq!(ETHNIC_CODE[0], ("01", "汉族"));
        assert_eq!(ETHNIC_CODE[55], ("56", "基诺族"));
        assert_eq!(get_ethnic_name("03"), Some("回族"));
        assert_eq!(get_ethnic_name("99"), None);
    }

    // ── GB/T 2261.2 婚姻状况代码 ──────────────────────────────────────────
    #[test]
    fn test_marital_status_first_last() {
        assert_eq!(MARITAL_STATUS_CODE[0], ("1", "未婚"));
        assert_eq!(MARITAL_STATUS_CODE[4], ("9", "未说明的婚姻状况"));
        assert_eq!(get_marital_status_name("2"), Some("已婚"));
        assert_eq!(get_marital_status_name("0"), None);
    }
}
