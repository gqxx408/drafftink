# 国标（GB/T）代码表核对清单 · STANDARDS_VERIFICATION

> 生成时间：2026-08-12
> 适用范围：`drafftink-core` 中
> `src/utils/gb_standard_codes.rs` / `gb_industry_codes.rs` / `gb_language_codes.rs`
> 目的：对齐（verify）已硬编码代码表与官方标准，闭环数据来源风险。

---

## 0. 图例

| 标记 | 含义 |
|------|------|
| `[PDF_EXTRACTED]` | 源 PDF 文本层含中文，数据由脚本从 PDF 原文提取，可信度最高 |
| `[PUBLIC_DOMAIN_REFERENCE]` | 源 PDF 为 CID/图像（文本层零中文、无 OCR），数据取自对应标准**已发布的代码表**，需人工核对 |
| `[SYSTEM_INTERNAL]` | 系统内部约定，非标准数据 |

---

## 1. 七张代码表总览

| # | 代码表（常量） | 标准 | 来源状态 | 条目数 | 版本标识 |
|---|---------------|------|----------|--------|----------|
| 1 | `EDUCATION_LEVEL_CODE` | GB/T 4658-2006 | `[PUBLIC_DOMAIN_REFERENCE]` | 8 | `PublicDomainSnapshot-2026-08` |
| 2 | `DEGREE_CODE` | GB/T 6864-2003 | `[PUBLIC_DOMAIN_REFERENCE]` | 4 | `PublicDomainSnapshot-2026-08` |
| 3 | `TECH_POSITION_CODE` | GB/T 8561-2001 | `[PUBLIC_DOMAIN_REFERENCE]` | 24 | `PublicDomainSnapshot-2026-08` |
| 4 | `ETHNIC_CODE` | GB/T 3304-1991 | `[PUBLIC_DOMAIN_REFERENCE]` | 56 | `PublicDomainSnapshot-2026-08` |
| 5 | `MARITAL_STATUS_CODE` | GB/T 2261.2-2003 | `[PUBLIC_DOMAIN_REFERENCE]` | 5 | `PublicDomainSnapshot-2026-08` |
| 6 | `INDUSTRY_*` (SECTION/DIVISION/CLASS) | GB/T 4754-2017 | `[PDF_EXTRACTED]` | 20 / 97 / 1638 | `PdfExtracted-GBT4754-2017` |
| 7 | `LANGUAGE_CODE` | GB/T 4881-1985 | `[PUBLIC_DOMAIN_REFERENCE]` ⚠️ | 21 | `PublicDomainSnapshot-2026-08` |

> 结论：**7 张表中仅 1 张（GB/T 4754）为机器提取，其余 6 张均为公共资源参考，须人工核对。**

---

## 2. 逐表核查

> 规则：对 `[PUBLIC_DOMAIN_REFERENCE]` 表列出**前 5 条 + 后 5 条**便于抽样核对；小表不足 10 条则全列。

### 1. `EDUCATION_LEVEL_CODE` — GB/T 4658-2006 `[PUBLIC_DOMAIN_REFERENCE]`
- Version: `PublicDomainSnapshot-2026-08`
- 前 5 条：`01 研究生` · `02 大学本科` · `03 大学专科` · `04 中专` · `05 高中`
- 后 5 条（共 8 条，与前 5 重叠）：`04 中专` · `05 高中` · `06 初中` · `07 小学` · `08 其他`

### 2. `DEGREE_CODE` — GB/T 6864-2003 `[PUBLIC_DOMAIN_REFERENCE]`
- Version: `PublicDomainSnapshot-2026-08`
- 全表（共 4 条）：`001 名誉博士学位` · `011 博士` · `012 硕士` · `021 学士`

### 3. `TECH_POSITION_CODE` — GB/T 8561-2001 `[PUBLIC_DOMAIN_REFERENCE]`
- Version: `PublicDomainSnapshot-2026-08`
- 前 5 条：`01 高等学校教师` · `02 中等专业学校教师` · `03 技工学校教师` · `04 中学教师` · `05 小学教师`
- 后 5 条：`20 体育人员` · `21 艺术人员` · `22 海关人员` · `23 船舶技术人员` · `24 民用航空飞行技术人员`

### 4. `ETHNIC_CODE` — GB/T 3304-1991 `[PUBLIC_DOMAIN_REFERENCE]`
- Version: `PublicDomainSnapshot-2026-08`
- 前 5 条：`01 汉族` · `02 蒙古族` · `03 回族` · `04 藏族` · `05 维吾尔族`
- 后 5 条：`52 鄂伦春族` · `53 赫哲族` · `54 门巴族` · `55 珞巴族` · `56 基诺族`

### 5. `MARITAL_STATUS_CODE` — GB/T 2261.2-2003 `[PUBLIC_DOMAIN_REFERENCE]`
- Version: `PublicDomainSnapshot-2026-08`
- 全表（共 5 条）：`1 未婚` · `2 已婚` · `3 丧偶` · `4 离婚` · `9 未说明的婚姻状况`

### 6. `INDUSTRY_*` — GB/T 4754-2017 `[PDF_EXTRACTED]`
- Version: `PdfExtracted-GBT4754-2017`（机器提取，提供前/后 5 抽样）
- **SECTION**（门类，20）：前 5 `A 农、林、牧、渔业` / `B 采矿业` / `C 制造业` / `D 电力、热力、燃气及水生产和供应业` / `E 建筑业`；后 5 `P 教育` / `Q 卫生和社会工作` / `R 文化、体育和娱乐业` / `S 公共管理、社会保障和社会组织` / `T 国际组织`
- **DIVISION**（大类，97）：前 5 `01 农业` / `02 林业` / `03 畜牧业` / `04 渔业` / `05 农`；后 5 `93 人民政协` / `94 社会保障` / `95 群众团体` / `96 基层群众自治组织及其他组织` / `97 国际组织`
- **CLASS**（小类，1638）：前 5 `0111 稻谷种植` / `0112 小麦种植` / `0113 玉米种植` / `0114 甘蔗的种植` / `0115 烟草的种植`；后 5 `9620 村民自治组织` / `9700 家庭作为家政人员雇主的活动` / `9810 未加区分的私人家庭自用物品生产活动` / `9820 未加区分的私人家庭自我服务活动` / `9900 国际组织和机构的活动`

### 7. `LANGUAGE_CODE` — GB/T 4881-1985 `[PUBLIC_DOMAIN_REFERENCE]` ⚠️
- Version: `PublicDomainSnapshot-2026-08`
- 前 5 条：`1 汉语` · `2 英语` · `3 法语` · `4 德语` · `5 日语`
- 后 5 条：`17 印地语` · `18 泰语` · `19 越南语` · `20 土耳其语` · `21 其他语言`

---

## 3. ⚠️ 重点风险：GB/T 4881-1985（语种代码）

> 这是整套代码表中**不确定性最高**的一张，已按 CTO 要求**冻结**并加粗 TODO。

- **极度老旧**：1985 年标准，数字码（如 `1=汉语`）与现代国际化惯例不兼容。
- **混用现实**：教育系统统计常混用 GB/T 4881-1985（数字码）与 **GB/T 4880.1-2005**（2 字母码，如 `zh` / `CN`）。
- **本硬编码风险**：数字↔名称映射**未经官方 PDF 逐字节核对**，仅取自已发布常见语种表。
- **冻结声明**：代码层已加注
  `/// **TODO: Verify against Ministry of Education latest directory before production use.**`
  及 `get_language_name()` 上的 ⚠️ 提示。
- **建议**：
  1. 国际化**新增**字段优先采用 `zh` / `CN`（GB/T 4880.1-2005）；
  2. 本表仅作**历史/存量数据兼容**，新业务不要依赖其数字码；
  3. 上线前必须对照教育部最新目录逐项复核。

---

## 4. 防御性编程状态

### 4.1 版本化（指令 2）
- 所有 `[PUBLIC_DOMAIN_REFERENCE]` 表已加注 `Version: PublicDomainSnapshot-2026-08` + `SourceStatus`。
- `[PDF_EXTRACTED]` 表（GB/T 4754）加注 `Version: PdfExtracted-GBT4754-2017`。
- **替换策略**：取得官方可提取 PDF 后，只需替换 `const` 数组内容，业务查询代码（`get_*_name`）无需改动。

### 4.2 查询性能（指令 3）
- **现状改进**：`gb_industry_codes.rs` 中 1638 条小类的线性 `iter().find()` 已升级为
  `OnceLock<HashMap>` 惰性构建的 **O(1)** 查找；`INDUSTRY_*` const 数组仍为权威数据源。
- **phf 评估**：
  - `phf`（Perfect Hash Function）可在编译期生成零运行时分配的常量哈希表，理论最优；
  - 但本场景为"**初始化加载后高频查询**"，`OnceLock<HashMap>` 已满足 O(1)，
    且**不引入额外依赖 / 无需 build.rs**，与项目"零新增依赖"约束一致；
  - 若未来要求**零堆分配 / 纯 const 表**，可切换为 `phf`（新增 `phf` 依赖 + 由 `const` 数据生成 `phf_map!`），
    查询 API 保持不变。取舍说明已写入 `gb_industry_codes.rs` 注释。

---

## 5. 附录：基础代码表（来自更早的提取任务）

| 代码表 | 标准 | 来源状态 |
|--------|------|----------|
| `PROVINCE_CODE` | GB/T 2260-2007 | `[PDF_EXTRACTED]` |
| `SCHOOL_TYPE_CODE` | GB/T 33782-2017 | `[PDF_EXTRACTED]` |
| `URBAN_RURAL_CODE` | GB/T 33782-2017 | `[PDF_EXTRACTED]` |
| `GenderCode` | GB/T 2261.1-2003 | `[PUBLIC_DOMAIN_REFERENCE]`（稳定 4 值枚举 0/1/2/9） |
| `YesNoCode` | 系统内部约定 | `[SYSTEM_INTERNAL]` |

---

## 6. 下一步 Action Plan（建议给 WorkBuddy）

1. **获取可提取源**：取得含 ToUnicode 映射 / 可复制文本的标准 PDF，对 6 张 `[PUBLIC_DOMAIN_REFERENCE]` 表重新提取核对。
2. **优先复核高危项**：GB/T 4881（语种，数字映射）与 GB/T 8561（24 系列级）逐条比对。
3. **性能升级（可选）**：若需零堆分配，将 `OnceLock<HashMap>` 切换为 `phf` 编译期表（新增 `phf` 依赖 + build.rs）。
4. **国际化规范**：新字段优先 `GB/T 4880.1-2005`（`zh`/`CN`），与 4881 并存时以 4880.1 为准。
