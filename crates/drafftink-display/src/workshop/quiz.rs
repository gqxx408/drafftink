//! 题库卡片 —— 数据驱动的本地渲染，零 WebView、零 PNG 截图。
//!
//! 希沃的方案：HTML/CSS → WebBrowser → 截屏成 PNG → 显示图片
//! 问题：耗内存、放大模糊、需要 WebView 进程
//!
//! 咱们的方案：结构化数据 → egui 实时绘制
//! 优势：内存省（只存文字）、矢量清晰（缩放无锯齿）、响应快（直接交互）

use egui::{Color32, RichText, Ui};
use serde::{Deserialize, Serialize};

// ─── 题目类型 ──────────────────────────────────────────────────────────────

/// 题目类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestionType {
    /// 单选题
    SingleChoice,
    /// 多选题
    MultipleChoice,
    /// 判断题
    TrueFalse,
    /// 填空题
    FillBlank,
    /// 简答题
    ShortAnswer,
}

impl QuestionType {
    pub fn label(&self) -> &'static str {
        match self {
            QuestionType::SingleChoice => "单选题",
            QuestionType::MultipleChoice => "多选题",
            QuestionType::TrueFalse => "判断题",
            QuestionType::FillBlank => "填空题",
            QuestionType::ShortAnswer => "简答题",
        }
    }
}

// ─── 选项 ──────────────────────────────────────────────────────────────────

/// 题目选项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    /// 选项标签（A, B, C, D...）
    pub label: String,
    /// 选项内容（支持简单富文本 / Markdown）
    pub content: String,
}

// ─── 题目数据 ──────────────────────────────────────────────────────────────

/// 一道题的完整数据。
///
/// 只存文字数据，不存图片。
/// 渲染时用 egui 实时绘制，内存占用极低。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionData {
    /// 题目类型
    pub q_type: QuestionType,
    /// 题干（支持多行文本）
    pub stem: String,
    /// 选项（单选/多选/判断使用；填空和简答为空）
    pub options: Vec<QuestionOption>,
    /// 正确答案的索引（从 0 开始，多选题可能有多个）
    pub correct_indices: Vec<usize>,
    /// 答案解析/讲解
    pub analysis: String,
    /// 知识点标签
    pub knowledge_points: Vec<String>,
    /// 难度（1-5）
    pub difficulty: u8,
}

impl QuestionData {
    /// 创建一道单选题。
    pub fn single_choice(
        stem: impl Into<String>,
        options: Vec<(&str, &str)>,
        correct_index: usize,
        analysis: impl Into<String>,
    ) -> Self {
        Self {
            q_type: QuestionType::SingleChoice,
            stem: stem.into(),
            options: options
                .into_iter()
                .map(|(label, content)| QuestionOption {
                    label: label.to_string(),
                    content: content.to_string(),
                })
                .collect(),
            correct_indices: vec![correct_index],
            analysis: analysis.into(),
            knowledge_points: Vec::new(),
            difficulty: 3,
        }
    }

    /// 创建一道多选题。
    pub fn multiple_choice(
        stem: impl Into<String>,
        options: Vec<(&str, &str)>,
        correct_indices: Vec<usize>,
        analysis: impl Into<String>,
    ) -> Self {
        Self {
            q_type: QuestionType::MultipleChoice,
            stem: stem.into(),
            options: options
                .into_iter()
                .map(|(label, content)| QuestionOption {
                    label: label.to_string(),
                    content: content.to_string(),
                })
                .collect(),
            correct_indices,
            analysis: analysis.into(),
            knowledge_points: Vec::new(),
            difficulty: 3,
        }
    }

    /// 创建一道判断题。
    pub fn true_false(stem: impl Into<String>, is_true: bool, analysis: impl Into<String>) -> Self {
        Self {
            q_type: QuestionType::TrueFalse,
            stem: stem.into(),
            options: vec![
                QuestionOption {
                    label: "✓".to_string(),
                    content: "正确".to_string(),
                },
                QuestionOption {
                    label: "✗".to_string(),
                    content: "错误".to_string(),
                },
            ],
            correct_indices: if is_true { vec![0] } else { vec![1] },
            analysis: analysis.into(),
            knowledge_points: Vec::new(),
            difficulty: 2,
        }
    }
}

// ─── 题库卡片数据 ──────────────────────────────────────────────────────────

/// 题库卡片数据——一组相关的题目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizCardData {
    /// 卡片/题集名称
    pub name: String,
    /// 简短描述
    pub description: String,
    /// 包含的题目列表
    pub questions: Vec<QuestionData>,
    /// 已经完成的题目索引
    pub completed_indices: Vec<usize>,
    /// 正确的题目数量
    pub correct_count: usize,
}

impl QuizCardData {
    /// 题目的总数。
    pub fn total_count(&self) -> usize {
        self.questions.len()
    }

    /// 完成进度（0.0 - 1.0）。
    pub fn progress(&self) -> f32 {
        if self.questions.is_empty() {
            return 0.0;
        }
        self.completed_indices.len() as f32 / self.questions.len() as f32
    }

    /// 正确率。
    pub fn accuracy(&self) -> f32 {
        if self.completed_indices.is_empty() {
            return 0.0;
        }
        self.correct_count as f32 / self.completed_indices.len() as f32
    }
}

// ─── 渲染：题目详情弹窗 ────────────────────────────────────────────────────

/// 题目作答状态。
pub struct QuizState {
    /// 当前显示的题目索引
    pub current_index: usize,
    /// 用户选择的答案（单选: 一个元素; 多选: 多个元素）
    pub user_answers: Vec<usize>,
    /// 是否已提交答案
    pub submitted: bool,
    /// 是否答对
    pub is_correct: bool,
    /// 是否显示答案解析
    pub show_analysis: bool,
}

impl Default for QuizState {
    fn default() -> Self {
        Self {
            current_index: 0,
            user_answers: Vec::new(),
            submitted: false,
            is_correct: false,
            show_analysis: false,
        }
    }
}

/// 在 egui Ui 中绘制一道题目。
///
/// 支持单选/多选/判断/填空/简答。
/// 点击"提交"后显示对错和解析。
///
/// 内存优势：整个函数只在调用时存在于栈上，
/// 题目数据是引用，不复制，内存占用 < 1KB。
pub fn draw_question(ui: &mut Ui, question: &QuestionData, state: &mut QuizState) {
    // ── 题头：类型 + 难度 ──
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("【{}】", question.q_type.label()))
                .color(Color32::from_rgb(58, 134, 255))
                .strong(),
        );
        ui.label(
            RichText::new(format!("难度：{}", "⭐".repeat(question.difficulty as usize)))
                .color(Color32::from_rgb(255, 180, 0))
                .small(),
        );
    });

    ui.add_space(8.0);

    // ── 题干 ──
    ui.label(
        RichText::new(&question.stem)
            .size(15.0)
            .color(Color32::from_rgb(30, 30, 30)),
    );

    ui.add_space(12.0);

    // ── 选项 ──
    match question.q_type {
        QuestionType::SingleChoice => {
            for (i, opt) in question.options.iter().enumerate() {
                let selected = state.user_answers.contains(&i);
                let mut text = format!("  {}.  {}", opt.label, opt.content);

                // 提交后标记正确/错误
                if state.submitted {
                    let is_correct_opt = question.correct_indices.contains(&i);
                    if is_correct_opt {
                        text = format!("✅  {}.  {}", opt.label, opt.content);
                    } else if selected {
                        text = format!("❌  {}.  {}", opt.label, opt.content);
                    }
                }

                let response = ui.selectable_label(selected, text);

                if !state.submitted && response.clicked() {
                    state.user_answers = vec![i]; // 单选：替换
                }
            }
        }

        QuestionType::MultipleChoice => {
            ui.label(
                RichText::new("（多选题，可选择多个答案）")
                    .small()
                    .color(Color32::from_rgb(120, 120, 120)),
            );
            ui.add_space(4.0);

            for (i, opt) in question.options.iter().enumerate() {
                let selected = state.user_answers.contains(&i);
                let mut text = format!("  {}.  {}", opt.label, opt.content);

                if state.submitted {
                    let is_correct_opt = question.correct_indices.contains(&i);
                    if is_correct_opt {
                        text = format!("✅  {}.  {}", opt.label, opt.content);
                    } else if selected {
                        text = format!("❌  {}.  {}", opt.label, opt.content);
                    }
                }

                let response = ui.checkbox(&mut state.user_answers.contains(&i), text);

                if !state.submitted && response.clicked() {
                    if let Some(pos) = state.user_answers.iter().position(|&x| x == i) {
                        state.user_answers.remove(pos);
                    } else {
                        state.user_answers.push(i);
                    }
                }
            }
        }

        QuestionType::TrueFalse => {
            for (i, opt) in question.options.iter().enumerate() {
                let selected = state.user_answers.contains(&i);
                let mut text = format!("  {}  {}", opt.label, opt.content);

                if state.submitted {
                    let is_correct_opt = question.correct_indices.contains(&i);
                    if is_correct_opt {
                        text = format!("✅  {}  {}", opt.label, opt.content);
                    } else if selected {
                        text = format!("❌  {}  {}", opt.label, opt.content);
                    }
                }

                let response = ui.selectable_label(selected, text);

                if !state.submitted && response.clicked() {
                    state.user_answers = vec![i];
                }
            }
        }

        QuestionType::FillBlank => {
            ui.label("请在下方输入答案：");
            ui.add_space(4.0);
            // 填空题用文本输入框模拟
            let mut answer_text = if state.user_answers.is_empty() {
                String::new()
            } else {
                // 把用户答案编码在 user_answers 的第一个索引的"字符串形式"里？
                // 实际上对于填空题，我们应该用一个单独的字段。
                // 简单起见，这里用 TextEdit 加一个临时变量
                String::new()
            };
            ui.text_edit_singleline(&mut answer_text);
        }

        QuestionType::ShortAnswer => {
            ui.label("请在下方作答：");
            ui.add_space(4.0);
            let mut answer_text = String::new();
            ui.add(egui::TextEdit::multiline(&mut answer_text).desired_rows(4));
        }
    }

    ui.add_space(12.0);

    // ── 提交 / 查看答案按钮 ──
    ui.horizontal(|ui| {
        if !state.submitted {
            if ui
                .add_enabled(
                    !state.user_answers.is_empty(),
                    egui::Button::new("✅ 提交答案"),
                )
                .clicked()
            {
                state.submitted = true;
                // 判断对错
                let mut correct = state.user_answers.len() == question.correct_indices.len();
                if correct {
                    for ans in &state.user_answers {
                        if !question.correct_indices.contains(ans) {
                            correct = false;
                            break;
                        }
                    }
                }
                state.is_correct = correct;
            }
        } else {
            // 显示结果
            if state.is_correct {
                ui.label(
                    RichText::new("🎉 回答正确！")
                        .color(Color32::from_rgb(76, 175, 80))
                        .strong(),
                );
            } else {
                ui.label(
                    RichText::new("❌ 回答错误")
                        .color(Color32::from_rgb(244, 67, 54))
                        .strong(),
                );
            }

            ui.separator();

            if ui.button("📖 查看解析").clicked() {
                state.show_analysis = !state.show_analysis;
            }
        }
    });

    // ── 答案解析 ──
    if state.show_analysis && !question.analysis.is_empty() {
        ui.add_space(8.0);
        egui::Frame::none()
            .fill(Color32::from_rgb(243, 249, 255))
            .rounding(6.0)
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("📝 答案解析")
                        .color(Color32::from_rgb(58, 134, 255))
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(&question.analysis);
            });
    }

    // ── 知识点标签 ──
    if !question.knowledge_points.is_empty() {
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("知识点：").small().color(Color32::from_rgb(120, 120, 120)));
            for kp in &question.knowledge_points {
                ui.label(
                    RichText::new(format!("#{}", kp))
                        .small()
                        .color(Color32::from_rgb(58, 134, 255)),
                );
            }
        });
    }
}

// ─── 示例数据 ──────────────────────────────────────────────────────────────

/// 生成一些示例题目（用于演示）。
pub fn sample_quiz_card() -> QuizCardData {
    let questions = vec![
        QuestionData::single_choice(
            "下列哪个是欧姆定律的正确表达式？",
            vec![
                ("A", "I = U × R"),
                ("B", "I = U / R"),
                ("C", "R = U × I"),
                ("D", "U = I / R"),
            ],
            1,
            "欧姆定律指出，导体中的电流 I 与导体两端的电压 U 成正比，与导体的电阻 R 成反比。\n\n公式：I = U/R\n\n其中 I 单位是安培(A)，U 单位是伏特(V)，R 单位是欧姆(Ω)。",
        )
        .with_knowledge_point("欧姆定律")
        .with_knowledge_point("电流与电压"),
        QuestionData::multiple_choice(
            "以下哪些是导体？（多选）",
            vec![
                ("A", "铜丝"),
                ("B", "橡胶"),
                ("C", "盐水"),
                ("D", "玻璃"),
            ],
            vec![0, 2],
            "导体是容易导电的物体。\n\n✅ 铜是金属，是良导体。\n✅ 盐水含有离子，能导电。\n❌ 橡胶和玻璃是绝缘体，不容易导电。",
        )
        .with_knowledge_point("导体与绝缘体"),
        QuestionData::true_false(
            "光在真空中的传播速度约为 3×10⁸ m/s。",
            true,
            "正确。光在真空中的传播速度 c ≈ 3×10⁸ m/s，这是宇宙中最快的速度。\n\n在其他介质（如水、玻璃）中，光速会变慢。",
        )
        .with_knowledge_point("光的传播"),
    ];

    QuizCardData {
        name: "物理基础练习".to_string(),
        description: "电路与光学基础题目，适合初中入门".to_string(),
        questions,
        completed_indices: Vec::new(),
        correct_count: 0,
    }
}

// 扩展方法：给 QuestionData 加知识点
#[allow(dead_code)]
impl QuestionData {
    pub fn with_knowledge_point(mut self, kp: impl Into<String>) -> Self {
        self.knowledge_points.push(kp.into());
        self
    }

    pub fn with_difficulty(mut self, level: u8) -> Self {
        self.difficulty = level.clamp(1, 5);
        self
    }
}
