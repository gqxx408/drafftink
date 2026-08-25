//! # 标准代码体系
//!
//! 集成 JY/T 1002-2012 引用的 GB/T 系列与 JY/T 1001 基础代码。
//!
//! 每个代码表以 [`CodeTable`] 描述，由 [`ALL_CODE_TABLES`] 统一登记。
//! [`validate_code`] 提供「取值是否合法」的单一入口，供 [`FieldDef::validate`](super::types::FieldDef::validate) 调用。
//!
//! 设计原则：
//! - 枚举型代码（性别、民族、政治面貌…）以 `(代码, 含义)` 静态表登记，双向可查；
//! - 格式型代码（国籍 3 位字母、邮政编码 6 位数字、统一社会信用代码 18 位）以格式校验替代穷举。

/// 一张代码表的定义。
#[derive(Debug, Clone, Copy)]
pub struct CodeTable {
    /// 代码表标识符（被 `FieldDef.code_ref` 引用）。
    pub id: &'static str,
    /// 来源标准，如 `GB/T 2261.1`。
    pub standard: &'static str,
    /// 代码表名称，如「性别代码」。
    pub name: &'static str,
    /// 枚举条目 `(代码, 含义)`；格式型代码表此项为空，改用 [`kind`] 描述。
    pub entries: &'static [(&'static str, &'static str)],
    /// 代码表类别，决定校验方式。
    pub kind: CodeKind,
}

/// 代码表类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeKind {
    /// 枚举型：取值必须落在 `entries` 中。
    Enumerated,
    /// 格式型：取值须满足固定格式（见描述）。
    Format,
}

// ── JY/T 1004 普通中小学校管理信息扩展代码表 ─────────────────────────────────────

/// 学校办别代码（JY/T 1001）。
pub const JYT_1001_SCHOOL_RUN_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_SCHOOL_RUN_TYPE",
    standard: "JY/T 1001",
    name: "学校办别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("10", "教育部门办"),
        ("20", "其他部门办"),
        ("30", "地方企业办"),
        ("40", "事业单位办"),
        ("50", "军队办"),
        ("60", "集体办"),
        ("70", "民办"),
        ("80", "中外合作办"),
        ("90", "其他"),
    ],
};

/// 考试方式代码（JY/T 1001）。
pub const JYT_1001_EXAM_METHOD: CodeTable = CodeTable {
    id: "JYT_1001_EXAM_METHOD",
    standard: "JY/T 1001",
    name: "考试方式代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "开卷"),
        ("2", "闭卷"),
        ("3", "口试"),
        ("4", "实操"),
        ("9", "其他"),
    ],
};

/// 是否标志代码（JY/T 1001，通用逻辑标志）。
pub const JYT_1001_YES_NO: CodeTable = CodeTable {
    id: "JYT_1001_YES_NO",
    standard: "JY/T 1001",
    name: "是否标志代码",
    kind: CodeKind::Enumerated,
    entries: &[("0", "否"), ("1", "是")],
};

/// 课程类型代码（JY/T 1001 / JY/T 1004 课程类）。
pub const JYT_1001_COURSE_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_COURSE_TYPE",
    standard: "JY/T 1001",
    name: "课程类型代码",
    kind: CodeKind::Enumerated,
    entries: &[("1", "必修"), ("2", "选修"), ("3", "活动"), ("9", "其他")],
};

/// 教材类型代码（JY/T 1001）。
pub const JYT_1001_TEXTBOOK_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_TEXTBOOK_TYPE",
    standard: "JY/T 1001",
    name: "教材类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "国家教材"),
        ("2", "地方教材"),
        ("3", "校本教材"),
        ("9", "其他"),
    ],
};

/// 教学计划类型代码（JY/T 1001）。
pub const JYT_1001_TEACH_PLAN_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_TEACH_PLAN_TYPE",
    standard: "JY/T 1001",
    name: "教学计划类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "学年计划"),
        ("2", "学期计划"),
        ("3", "单元计划"),
        ("9", "其他"),
    ],
};

/// 星期代码（JY/T 1001 / GB/T 7408 周几）。
pub const JYT_1001_WEEK: CodeTable = CodeTable {
    id: "JYT_1001_WEEK",
    standard: "JY/T 1001",
    name: "星期代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "星期一"),
        ("2", "星期二"),
        ("3", "星期三"),
        ("4", "星期四"),
        ("5", "星期五"),
        ("6", "星期六"),
        ("7", "星期日"),
    ],
};

/// 节次代码（JY/T 1001，上课节次）。
pub const JYT_1001_PERIOD: CodeTable = CodeTable {
    id: "JYT_1001_PERIOD",
    standard: "JY/T 1001",
    name: "节次代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "第一节"),
        ("2", "第二节"),
        ("3", "第三节"),
        ("4", "第四节"),
        ("5", "第五节"),
        ("6", "第六节"),
        ("7", "第七节"),
        ("8", "第八节"),
    ],
};

/// 德育类型代码（JY/T 1001 / JY/T 1004 德育类）。
pub const JYT_1001_DEED_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_DEED_TYPE",
    standard: "JY/T 1001",
    name: "德育类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "思想品德"),
        ("2", "行为规范"),
        ("3", "社会实践"),
        ("4", "志愿服务"),
        ("9", "其他"),
    ],
};

/// 医疗保健类型代码（JY/T 1001 / JY/T 1004 体育卫生类）。
pub const JYT_1001_MEDICAL_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_MEDICAL_TYPE",
    standard: "JY/T 1001",
    name: "医疗保健类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "门诊"),
        ("2", "住院"),
        ("3", "体检"),
        ("4", "疫苗接种"),
        ("9", "其他"),
    ],
};

/// 体育运动类型代码（JY/T 1001 / JY/T 1004 体育类）。
pub const JYT_1001_SPORT_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_SPORT_TYPE",
    standard: "JY/T 1001",
    name: "体育运动类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "田径"),
        ("2", "球类"),
        ("3", "体操"),
        ("4", "水上运动"),
        ("5", "武术"),
        ("9", "其他"),
    ],
};

/// 综合素质评价类型代码（JY/T 1001 / JY/T 1004 评价类）。
pub const JYT_1001_EVAL_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_EVAL_TYPE",
    standard: "JY/T 1001",
    name: "综合素质评价类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "品德发展"),
        ("2", "学业发展"),
        ("3", "身心发展"),
        ("4", "审美素养"),
        ("5", "劳动实践"),
    ],
};

/// 毕结业代码（JY/T 1001）。
pub const JYT_1001_GRAD_CODE: CodeTable = CodeTable {
    id: "JYT_1001_GRAD_CODE",
    standard: "JY/T 1001",
    name: "毕结业代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "毕业"),
        ("2", "结业"),
        ("3", "肄业"),
        ("4", "休学"),
        ("9", "其他"),
    ],
};

/// 岗位代码（JY/T 1001 / JY/T 1004 教职工类）。
pub const JYT_1001_POST_CODE: CodeTable = CodeTable {
    id: "JYT_1001_POST_CODE",
    standard: "JY/T 1001",
    name: "岗位代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "教学岗"),
        ("2", "管理岗"),
        ("3", "教辅岗"),
        ("4", "工勤岗"),
        ("9", "其他"),
    ],
};

/// 职务代码（JY/T 1001 / JY/T 1004 教职工类）。
pub const JYT_1001_DUTY_CODE: CodeTable = CodeTable {
    id: "JYT_1001_DUTY_CODE",
    standard: "JY/T 1001",
    name: "职务代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "校长"),
        ("2", "副校长"),
        ("3", "教务主任"),
        ("4", "年级组长"),
        ("5", "教研组长"),
        ("9", "其他"),
    ],
};

/// 公文类型代码（JY/T 1001 / JY/T 1004 办公管理类）。
pub const JYT_1001_DOC_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_DOC_TYPE",
    standard: "JY/T 1001",
    name: "公文类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("10", "决议"),
        ("20", "决定"),
        ("30", "命令"),
        ("40", "公报"),
        ("50", "公告"),
        ("60", "通告"),
        ("70", "意见"),
        ("80", "通知"),
        ("90", "报告"),
        ("91", "请示"),
        ("92", "批复"),
        ("99", "其他"),
    ],
};

/// 紧急程度代码（JY/T 1001 / JY/T 1004 办公管理类）。
pub const JYT_1001_URGENCY: CodeTable = CodeTable {
    id: "JYT_1001_URGENCY",
    standard: "JY/T 1001",
    name: "紧急程度代码",
    kind: CodeKind::Enumerated,
    entries: &[("1", "特急"), ("2", "加急"), ("3", "平急"), ("9", "普通")],
};

/// 密级代码（GB/T 保密 — 涉密文件密级）。
pub const JYT_1001_SECRET_LEVEL: CodeTable = CodeTable {
    id: "JYT_1001_SECRET_LEVEL",
    standard: "GB/T 7156",
    name: "密级代码",
    kind: CodeKind::Enumerated,
    entries: &[("0", "非涉密"), ("1", "秘密"), ("2", "机密"), ("3", "绝密")],
};

/// 审批状态代码（JY/T 1004 办公管理 — ZXBG 审批流）。
pub const JYT_1001_APPROVAL_STATUS: CodeTable = CodeTable {
    id: "JYT_1001_APPROVAL_STATUS",
    standard: "JY/T 1004",
    name: "审批状态代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("00", "草稿"),
        ("10", "流转中"),
        ("20", "审批通过"),
        ("30", "审批驳回"),
        ("40", "已作废"),
    ],
};

/// 全部登记代码表。
pub const ALL_CODE_TABLES: &[CodeTable] = &[
    GBT_2261_1_GENDER,
    GBT_3304_ETHNICITY,
    GBT_2659_NATIONALITY,
    JYT_1001_ID_TYPE,
    JYT_1001_HEALTH,
    GBT_2261_2_MARITAL,
    GBT_4762_POLITICAL,
    JYT_1001_HMT,
    JYT_1001_SCHOOL_NATURE,
    JYT_1001_BLOOD,
    JYT_1001_TERM,
    JYT_1001_GRADE,
    JYT_1001_SUBJECT,
    JYT_1001_CLASS_TYPE,
    JYT_1001_ENROLL_TYPE,
    JYT_1001_STUDY_MODE,
    JYT_1001_STUDENT_CATEGORY,
    JYT_1001_STATUS_STATE,
    JYT_1001_SCORE_TYPE,
    JYT_1001_AWARD_TYPE,
    JYT_1001_AWARD_LEVEL,
    JYT_1001_PUNISH_TYPE,
    JYT_1001_PUNISH_LEVEL,
    JYT_1001_STAFF_CATEGORY,
    JYT_1001_STAFF_STATE,
    JYT_1001_STAFF_SOURCE,
    JYT_1001_TITLE_LEVEL,
    JYT_1001_HOUSE_TYPE,
    JYT_1001_EQUIP_TYPE,
    GBT_4658_EDUCATION,
    GBT_6864_DEGREE,
    GBT_8561_TITLE,
    GBT_POSTAL_CODE,
    GBT_CREDIT_CODE,
    JYT_1001_SCHOOL_RUN_TYPE,
    JYT_1001_EXAM_METHOD,
    JYT_1001_YES_NO,
    JYT_1001_COURSE_TYPE,
    JYT_1001_TEXTBOOK_TYPE,
    JYT_1001_TEACH_PLAN_TYPE,
    JYT_1001_WEEK,
    JYT_1001_PERIOD,
    JYT_1001_DEED_TYPE,
    JYT_1001_MEDICAL_TYPE,
    JYT_1001_SPORT_TYPE,
    JYT_1001_EVAL_TYPE,
    JYT_1001_GRAD_CODE,
    JYT_1001_POST_CODE,
    JYT_1001_DUTY_CODE,
    JYT_1001_DOC_TYPE,
    JYT_1001_URGENCY,
    JYT_1001_SECRET_LEVEL,
    JYT_1001_APPROVAL_STATUS,
];

// ── 枚举型代码表 ────────────────────────────────────────────────────────────

/// GB/T 2261.1 人的性别代码。
pub const GBT_2261_1_GENDER: CodeTable = CodeTable {
    id: "GBT_2261_1_GENDER",
    standard: "GB/T 2261.1",
    name: "人的性别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("0", "未知的性别"),
        ("1", "男性"),
        ("2", "女性"),
        ("9", "未说明的性别"),
    ],
};

/// GB/T 3304 民族代码（56 个民族）。
pub const GBT_3304_ETHNICITY: CodeTable = CodeTable {
    id: "GBT_3304_ETHNICITY",
    standard: "GB/T 3304",
    name: "中国各民族名称的罗马字母拼写法和代码",
    kind: CodeKind::Enumerated,
    entries: &[
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
    ],
};

/// GB/T 2659 世界各国和地区名称代码（ISO 3166-1 alpha-3）。
///
/// 取值为 3 位大写字母，此处仅登记常用项，并以格式校验兜底。
pub const GBT_2659_NATIONALITY: CodeTable = CodeTable {
    id: "GBT_2659_NATIONALITY",
    standard: "GB/T 2659",
    name: "世界各国和地区名称代码",
    kind: CodeKind::Format,
    entries: &[
        ("CHN", "中国"),
        ("HKG", "中国香港"),
        ("MAC", "中国澳门"),
        ("TWN", "中国台湾"),
        ("USA", "美国"),
        ("GBR", "英国"),
        ("JPN", "日本"),
        ("KOR", "韩国"),
        ("PRK", "朝鲜"),
        ("FRA", "法国"),
        ("DEU", "德国"),
        ("RUS", "俄罗斯"),
        ("CAN", "加拿大"),
        ("AUS", "澳大利亚"),
        ("SGP", "新加坡"),
    ],
};

/// JY/T 1001 身份证件类型代码。
pub const JYT_1001_ID_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_ID_TYPE",
    standard: "JY/T 1001",
    name: "身份证件类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("01", "居民身份证"),
        ("02", "军官证"),
        ("03", "士兵证"),
        ("04", "文职干部证"),
        ("05", "部队离退休证"),
        ("06", "香港身份证"),
        ("07", "澳门身份证"),
        ("08", "台湾居民来往大陆通行证"),
        ("09", "港澳台居民居住证"),
        ("10", "护照"),
        ("99", "其他"),
    ],
};

/// JY/T 1001 健康状况代码。
pub const JYT_1001_HEALTH: CodeTable = CodeTable {
    id: "JYT_1001_HEALTH",
    standard: "JY/T 1001",
    name: "健康状况代码",
    kind: CodeKind::Enumerated,
    entries: &[("1", "健康"), ("2", "残疾"), ("9", "其他")],
};

/// GB/T 2261.2 婚姻状况代码。
pub const GBT_2261_2_MARITAL: CodeTable = CodeTable {
    id: "GBT_2261_2_MARITAL",
    standard: "GB/T 2261.2",
    name: "婚姻状况代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "未婚"),
        ("2", "已婚"),
        ("3", "丧偶"),
        ("4", "离婚"),
        ("9", "未说明"),
    ],
};

/// GB/T 4762 政治面貌代码。
pub const GBT_4762_POLITICAL: CodeTable = CodeTable {
    id: "GBT_4762_POLITICAL",
    standard: "GB/T 4762",
    name: "政治面貌代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("01", "中国共产党党员"),
        ("02", "中国共产党预备党员"),
        ("03", "中国共产主义青年团团员"),
        ("04", "中国国民党革命委员会会员"),
        ("05", "中国民主同盟盟员"),
        ("06", "中国民主建国会会员"),
        ("07", "中国民主促进会会员"),
        ("08", "中国农工民主党党员"),
        ("09", "中国致公党党员"),
        ("10", "九三学社社员"),
        ("11", "台湾民主自治同盟盟员"),
        ("12", "无党派民主人士"),
        ("13", "群众"),
    ],
};

/// JY/T 1001 港澳台侨代码。
pub const JYT_1001_HMT: CodeTable = CodeTable {
    id: "JYT_1001_HMT",
    standard: "JY/T 1001",
    name: "港澳台侨代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("0", "非港澳台侨"),
        ("1", "香港同胞"),
        ("2", "澳门同胞"),
        ("3", "台湾同胞"),
        ("4", "华侨"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 学校办学类型（性质）代码。
pub const JYT_1001_SCHOOL_NATURE: CodeTable = CodeTable {
    id: "JYT_1001_SCHOOL_NATURE",
    standard: "JY/T 1001",
    name: "学校办学类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "学前教育"),
        ("2", "初等教育"),
        ("3", "中等教育"),
        ("4", "高等教育"),
        ("9", "其他教育"),
    ],
};

/// JY/T 1001 血型代码。
pub const JYT_1001_BLOOD: CodeTable = CodeTable {
    id: "JYT_1001_BLOOD",
    standard: "JY/T 1001",
    name: "血型代码",
    kind: CodeKind::Enumerated,
    entries: &[("A", "A 型"), ("B", "B 型"), ("AB", "AB 型"), ("O", "O 型")],
};

/// JY/T 1001 学期代码。
pub const JYT_1001_TERM: CodeTable = CodeTable {
    id: "JYT_1001_TERM",
    standard: "JY/T 1001",
    name: "学期代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "第一学期"),
        ("2", "第二学期"),
        ("3", "第三学期"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 年级代码。
pub const JYT_1001_GRADE: CodeTable = CodeTable {
    id: "JYT_1001_GRADE",
    standard: "JY/T 1001",
    name: "年级代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("01", "一年级"),
        ("02", "二年级"),
        ("03", "三年级"),
        ("04", "四年级"),
        ("05", "五年级"),
        ("06", "六年级"),
        ("07", "七年级"),
        ("08", "八年级"),
        ("09", "九年级"),
        ("10", "高一"),
        ("11", "高二"),
        ("12", "高三"),
    ],
};

/// JY/T 1001 学科（课程）代码（常用学科）。
pub const JYT_1001_SUBJECT: CodeTable = CodeTable {
    id: "JYT_1001_SUBJECT",
    standard: "JY/T 1001",
    name: "学科（课程）代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("01", "语文"),
        ("02", "数学"),
        ("03", "英语"),
        ("04", "物理"),
        ("05", "化学"),
        ("06", "生物"),
        ("07", "历史"),
        ("08", "地理"),
        ("09", "政治"),
        ("10", "思想品德"),
        ("11", "信息技术"),
        ("12", "体育"),
        ("13", "音乐"),
        ("14", "美术"),
        ("15", "科学"),
        ("16", "综合实践活动"),
    ],
};

/// JY/T 1001 班级类型代码。
pub const JYT_1001_CLASS_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_CLASS_TYPE",
    standard: "JY/T 1001",
    name: "班级类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "行政班"),
        ("2", "教学班"),
        ("3", "实验班"),
        ("4", "特长班"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 入学方式代码。
pub const JYT_1001_ENROLL_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_ENROLL_TYPE",
    standard: "JY/T 1001",
    name: "入学（招生）方式代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "统一招生"),
        ("2", "保送"),
        ("3", "特长生"),
        ("4", "择校"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 就读方式代码。
pub const JYT_1001_STUDY_MODE: CodeTable = CodeTable {
    id: "JYT_1001_STUDY_MODE",
    standard: "JY/T 1001",
    name: "就读方式代码",
    kind: CodeKind::Enumerated,
    entries: &[("1", "走读"), ("2", "寄宿"), ("9", "其他")],
};

/// JY/T 1001 学生类别代码。
pub const JYT_1001_STUDENT_CATEGORY: CodeTable = CodeTable {
    id: "JYT_1001_STUDENT_CATEGORY",
    standard: "JY/T 1001",
    name: "学生类别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "普通学生"),
        ("2", "留学生"),
        ("3", "函授生"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 学籍状态代码。
pub const JYT_1001_STATUS_STATE: CodeTable = CodeTable {
    id: "JYT_1001_STATUS_STATE",
    standard: "JY/T 1001",
    name: "学籍状态代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "在学"),
        ("2", "休学"),
        ("3", "退学"),
        ("4", "毕业"),
        ("5", "结业"),
        ("6", "肄业"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 成绩类型代码。
pub const JYT_1001_SCORE_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_SCORE_TYPE",
    standard: "JY/T 1001",
    name: "成绩类型代码",
    kind: CodeKind::Enumerated,
    entries: &[("1", "考试"), ("2", "考查"), ("3", "总评"), ("9", "其他")],
};

/// JY/T 1001 奖励类别代码。
pub const JYT_1001_AWARD_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_AWARD_TYPE",
    standard: "JY/T 1001",
    name: "奖励类别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "综合荣誉"),
        ("2", "学科竞赛"),
        ("3", "文体活动"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 奖励级别代码。
pub const JYT_1001_AWARD_LEVEL: CodeTable = CodeTable {
    id: "JYT_1001_AWARD_LEVEL",
    standard: "JY/T 1001",
    name: "奖励级别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "国家级"),
        ("2", "省级"),
        ("3", "市级"),
        ("4", "区县级"),
        ("5", "校级"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 惩处类别代码。
pub const JYT_1001_PUNISH_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_PUNISH_TYPE",
    standard: "JY/T 1001",
    name: "惩处类别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "警告"),
        ("2", "严重警告"),
        ("3", "记过"),
        ("4", "留校察看"),
        ("5", "开除学籍"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 惩处级别代码。
pub const JYT_1001_PUNISH_LEVEL: CodeTable = CodeTable {
    id: "JYT_1001_PUNISH_LEVEL",
    standard: "JY/T 1001",
    name: "惩处级别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "校级"),
        ("2", "区县级"),
        ("3", "地市级"),
        ("4", "省级"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 教职工类别代码。
pub const JYT_1001_STAFF_CATEGORY: CodeTable = CodeTable {
    id: "JYT_1001_STAFF_CATEGORY",
    standard: "JY/T 1001",
    name: "教职工类别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "专任教师"),
        ("2", "行政人员"),
        ("3", "教辅人员"),
        ("4", "工勤人员"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 教职工当前状态代码。
pub const JYT_1001_STAFF_STATE: CodeTable = CodeTable {
    id: "JYT_1001_STAFF_STATE",
    standard: "JY/T 1001",
    name: "教职工当前状态代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "在岗"),
        ("2", "离休"),
        ("3", "退休"),
        ("4", "离岗"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 教职工来源代码。
pub const JYT_1001_STAFF_SOURCE: CodeTable = CodeTable {
    id: "JYT_1001_STAFF_SOURCE",
    standard: "JY/T 1001",
    name: "教职工来源代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "毕业生"),
        ("2", "调任"),
        ("3", "招聘"),
        ("4", "军转"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 专业技术职务级别代码。
pub const JYT_1001_TITLE_LEVEL: CodeTable = CodeTable {
    id: "JYT_1001_TITLE_LEVEL",
    standard: "JY/T 1001",
    name: "专业技术职务级别代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "正高级"),
        ("2", "副高级"),
        ("3", "中级"),
        ("4", "初级"),
        ("9", "未定级"),
    ],
};

/// JY/T 1001 校舍场所类型代码。
pub const JYT_1001_HOUSE_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_HOUSE_TYPE",
    standard: "JY/T 1001",
    name: "校舍场所类型代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "教学及辅助用房"),
        ("2", "行政办公用房"),
        ("3", "生活用房"),
        ("4", "运动场地"),
        ("9", "其他"),
    ],
};

/// JY/T 1001 仪器设备分类代码。
pub const JYT_1001_EQUIP_TYPE: CodeTable = CodeTable {
    id: "JYT_1001_EQUIP_TYPE",
    standard: "JY/T 1001",
    name: "仪器设备分类代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("1", "通用设备"),
        ("2", "专用设备"),
        ("3", "文物陈列品"),
        ("4", "图书"),
        ("9", "其他"),
    ],
};

/// GB/T 4658 学历代码。
pub const GBT_4658_EDUCATION: CodeTable = CodeTable {
    id: "GBT_4658_EDUCATION",
    standard: "GB/T 4658",
    name: "学历代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("10", "博士研究生"),
        ("20", "硕士研究生"),
        ("30", "大学本科"),
        ("40", "大学专科"),
        ("50", "中等专业学校"),
        ("60", "高级中学"),
        ("70", "初级中学"),
        ("80", "小学"),
        ("90", "其他"),
    ],
};

/// GB/T 6864 学位代码。
pub const GBT_6864_DEGREE: CodeTable = CodeTable {
    id: "GBT_6864_DEGREE",
    standard: "GB/T 6864",
    name: "学位代码",
    kind: CodeKind::Enumerated,
    entries: &[("1", "博士"), ("2", "硕士"), ("3", "学士")],
};

/// GB/T 8561 专业技术职务代码（常用）。
pub const GBT_8561_TITLE: CodeTable = CodeTable {
    id: "GBT_8561_TITLE",
    standard: "GB/T 8561",
    name: "专业技术职务代码",
    kind: CodeKind::Enumerated,
    entries: &[
        ("0101", "教授"),
        ("0102", "副教授"),
        ("0103", "讲师"),
        ("0104", "助教"),
        ("0201", "研究员"),
        ("0202", "副研究员"),
        ("0203", "助理研究员"),
        ("0204", "研究实习员"),
        ("0301", "高级工程师"),
        ("0302", "工程师"),
        ("0303", "助理工程师"),
        ("0304", "技术员"),
        ("0401", "中学高级教师"),
        ("0402", "中学一级教师"),
        ("0403", "中学二级教师"),
        ("0404", "中学三级教师"),
        ("0501", "小学高级教师"),
        ("0502", "小学一级教师"),
        ("0503", "小学二级教师"),
        ("0504", "小学三级教师"),
        ("0901", "高级会计师"),
        ("0902", "会计师"),
        ("0903", "助理会计师"),
        ("0904", "会计员"),
    ],
};

/// GB/T 2260 行政区划（邮政编码）代码。取值为 6 位数字。
pub const GBT_POSTAL_CODE: CodeTable = CodeTable {
    id: "GBT_POSTAL_CODE",
    standard: "GB/T 2260",
    name: "中华人民共和国行政区划代码（邮政编码）",
    kind: CodeKind::Format,
    entries: &[],
};

/// GB 32100 统一社会信用代码。取值为 18 位（含校验位）。
pub const GBT_CREDIT_CODE: CodeTable = CodeTable {
    id: "GBT_CREDIT_CODE",
    standard: "GB 32100",
    name: "法人和其他组织统一社会信用代码",
    kind: CodeKind::Format,
    entries: &[],
};

/// 查找代码含义（双向：支持代码→含义）。返回 `None` 表示非法。
pub fn lookup_meaning(table_id: &str, code: &str) -> Option<&'static str> {
    let table = ALL_CODE_TABLES.iter().find(|t| t.id == table_id)?;
    table
        .entries
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, m)| *m)
}

/// 校验取值是否落在代码表内。
///
/// 返回 `None` 表示合法；返回 `Some(reason)` 表示不合法及原因。
pub fn validate_code(table_id: &str, value: &str) -> Option<&'static str> {
    let table = match ALL_CODE_TABLES.iter().find(|t| t.id == table_id) {
        Some(t) => t,
        None => return Some("未知代码表"),
    };

    match table.kind {
        CodeKind::Enumerated => {
            if table.entries.iter().any(|(c, _)| *c == value) {
                None
            } else {
                Some("取值不在代码表枚举范围内")
            }
        }
        CodeKind::Format => match table.id {
            "GBT_POSTAL_CODE" => {
                if value.len() == 6 && value.bytes().all(|b| b.is_ascii_digit()) {
                    None
                } else {
                    Some("邮政编码应为 6 位数字")
                }
            }
            "GBT_CREDIT_CODE" => {
                if value.len() == 18 && value.bytes().all(|b| b.is_ascii_alphanumeric()) {
                    None
                } else {
                    Some("统一社会信用代码应为 18 位字母数字")
                }
            }
            "GBT_2659_NATIONALITY" => {
                if value.len() == 3 && value.bytes().all(|b| b.is_ascii_uppercase()) {
                    None
                } else {
                    Some("国家/地区代码应为 3 位大写字母")
                }
            }
            _ => Some("未实现的格式代码表"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gender_enum() {
        assert_eq!(lookup_meaning("GBT_2261_1_GENDER", "1"), Some("男性"));
        assert!(validate_code("GBT_2261_1_GENDER", "1").is_none());
        assert!(validate_code("GBT_2261_1_GENDER", "7").is_some());
    }

    #[test]
    fn test_ethnicity_full_set() {
        assert_eq!(lookup_meaning("GBT_3304_ETHNICITY", "01"), Some("汉族"));
        assert_eq!(lookup_meaning("GBT_3304_ETHNICITY", "56"), Some("基诺族"));
        assert!(validate_code("GBT_3304_ETHNICITY", "56").is_none());
        assert!(validate_code("GBT_3304_ETHNICITY", "99").is_some());
    }

    #[test]
    fn test_format_codes() {
        assert!(validate_code("GBT_POSTAL_CODE", "100084").is_none());
        assert!(validate_code("GBT_POSTAL_CODE", "12345").is_some());
        assert!(validate_code("GBT_CREDIT_CODE", "91110108MA01ABCD23").is_none());
        assert!(validate_code("GBT_2659_NATIONALITY", "CHN").is_none());
        assert!(validate_code("GBT_2659_NATIONALITY", "chn").is_some());
    }

    #[test]
    fn test_all_tables_registered() {
        assert!(ALL_CODE_TABLES.len() >= 30);
    }
}
