//! 多学科工坊主界面 —— 卡片网格 + 分类标签 + 详情弹窗。
//!
//! 设计理念：
//! - 左侧：学科分类标签（垂直排列）
//! - 中央：卡片网格，自适应列数
//! - 右侧/弹窗：卡片详情（题目作答、实验操作等）
//!
//! 内存优化：
//! - 卡片只在可见时渲染（egui 自动处理裁剪）
//! - 题目数据用引用，不复制
//! - 缩略图使用 GPU 纹理，多个相同卡片共享

use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use uuid::Uuid;

use crate::workshop::cards::{CardElement, Subject, SubjectCard};
use crate::workshop::experiment::sample_experiment_card;
use crate::workshop::experiment::SimpleCircuitState;
use crate::workshop::quiz::{draw_question, sample_quiz_card, QuizState};

/// 工坊主状态。
pub struct Workshop {
    /// 所有卡片
    cards: Vec<CardElement>,
    /// 当前选中的学科分类（None = 全部）
    current_subject: Option<Subject>,
    /// 当前打开的卡片 ID（详情弹窗）
    open_card_id: Option<Uuid>,
    /// 题库答题状态（按卡片 ID 索引）
    quiz_states: std::collections::HashMap<Uuid, QuizState>,
    /// 电路实验状态（按卡片 ID 索引）
    circuit_states: std::collections::HashMap<Uuid, SimpleCircuitState>,
    /// 搜索关键词
    search_text: String,
}

impl Default for Workshop {
    fn default() -> Self {
        Self::new()
    }
}

impl Workshop {
    /// 创建工坊，加载示例卡片。
    pub fn new() -> Self {
        let mut cards = Vec::new();

        // 物理题库卡片
        let quiz_data = sample_quiz_card();
        let quiz_card = CardElement::new(
            quiz_data.name.clone(),
            Subject::Physics,
            SubjectCard::Quiz(quiz_data),
        )
        .with_difficulty(3)
        .with_tag("电路")
        .with_tag("光学");
        cards.push(quiz_card);

        // 物理实验卡片
        let exp_data = sample_experiment_card();
        let exp_card = CardElement::new(
            exp_data.name.clone(),
            Subject::Physics,
            SubjectCard::Experiment(exp_data),
        )
        .with_difficulty(2)
        .with_tag("串联电路");
        cards.push(exp_card);

        // 数学题库卡片
        let math_quiz = crate::workshop::quiz::QuizCardData {
            name: "几何基础练习".to_string(),
            description: "三角形、四边形、圆的基础题目".to_string(),
            questions: vec![
                crate::workshop::quiz::QuestionData::single_choice(
                    "三角形的内角和等于多少度？",
                    vec![
                        ("A", "90°"),
                        ("B", "180°"),
                        ("C", "270°"),
                        ("D", "360°"),
                    ],
                    1,
                    "三角形的内角和等于 180°。\n\n这是欧几里得几何的基本定理之一，可以通过平行线证明。",
                )
                .with_knowledge_point("三角形内角和"),
                crate::workshop::quiz::QuestionData::true_false(
                    "平行四边形的对角线互相平分。",
                    true,
                    "正确。平行四边形的对角线互相平分，即两条对角线的交点是各自的中点。",
                )
                .with_knowledge_point("平行四边形"),
            ],
            completed_indices: Vec::new(),
            correct_count: 0,
        };
        let math_card = CardElement::new(
            math_quiz.name.clone(),
            Subject::Math,
            SubjectCard::Quiz(math_quiz),
        )
        .with_difficulty(2)
        .with_tag("几何");
        cards.push(math_card);

        // 化学实验卡片
        let chem_exp = crate::workshop::experiment::ExperimentCardData {
            name: "氧气的制取".to_string(),
            description: "高锰酸钾加热分解制取氧气".to_string(),
            exp_type: crate::workshop::experiment::ExperimentType::Chemistry,
            difficulty: 3,
            step_count: 6,
            completed: false,
        };
        let chem_card = CardElement::new(
            chem_exp.name.clone(),
            Subject::Chemistry,
            SubjectCard::Experiment(chem_exp),
        )
        .with_difficulty(3)
        .with_tag("氧气");
        cards.push(chem_card);

        Self {
            cards,
            current_subject: None,
            open_card_id: None,
            quiz_states: std::collections::HashMap::new(),
            circuit_states: std::collections::HashMap::new(),
            search_text: String::new(),
        }
    }

    /// 渲染工坊主界面。
    pub fn ui(&mut self, ctx: &egui::Context) {
        // 顶部工具栏
        egui::TopBottomPanel::top("workshop_topbar")
            .max_height(50.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.heading("🎓 学科工坊");
                    ui.separator();

                    // 搜索框
                    ui.add_space(8.0);
                    let search_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.search_text)
                            .hint_text("🔍 搜索卡片...")
                            .desired_width(200.0),
                    );
                    let _ = search_resp;

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("共 {} 张卡片", self.cards.len()))
                                .small()
                                .color(Color32::from_rgb(120, 120, 120)),
                        );
                        ui.add_space(12.0);
                    });
                });
            });

        // 左侧学科分类
        egui::SidePanel::left("workshop_subjects")
            .default_width(120.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("学科分类")
                        .strong()
                        .color(Color32::from_rgb(80, 80, 80)),
                );
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // "全部" 选项
                let all_selected = self.current_subject.is_none();
                if ui.selectable_label(all_selected, "📦  全部").clicked() {
                    self.current_subject = None;
                }

                ui.add_space(4.0);

                // 各学科
                for subject in Subject::all() {
                    let count = self.cards.iter().filter(|c| c.subject == *subject).count();
                    if count == 0 {
                        continue; // 没有卡片的学科不显示
                    }
                    let selected = self.current_subject == Some(*subject);
                    let label = format!("{}  {}", subject.emoji(), subject.label());
                    if ui.selectable_label(selected, label).clicked() {
                        self.current_subject = Some(*subject);
                    }
                    // 在右侧显示数量
                    // ui.label(egui::RichText::new(format!("({})", count)).small());
                }
            });

        // 中央卡片网格
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(12.0);

            // 筛选卡片（收集索引，避免借用冲突）
            let filtered: Vec<usize> = self
                .cards
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    // 学科筛选
                    if let Some(subject) = self.current_subject {
                        if c.subject != subject {
                            return false;
                        }
                    }
                    // 搜索筛选
                    if !self.search_text.is_empty() {
                        let search = self.search_text.to_lowercase();
                        if !c.title.to_lowercase().contains(&search)
                            && !c.tags.iter().any(|t| t.to_lowercase().contains(&search))
                        {
                            return false;
                        }
                    }
                    true
                })
                .map(|(i, _)| i)
                .collect();

            if filtered.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.label(
                        egui::RichText::new("暂无卡片")
                            .size(20.0)
                            .color(Color32::from_rgb(180, 180, 180)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("换个分类或搜索词试试？")
                            .small()
                            .color(Color32::from_rgb(150, 150, 150)),
                    );
                });
                return;
            }

            // 自适应列数：根据可用宽度计算
            let available_width = ui.available_width();
            let card_width = 240.0;
            let card_height = 180.0;
            let spacing = 16.0;
            let columns = ((available_width + spacing) / (card_width + spacing)).floor() as usize;
            let columns = columns.max(1);

            // 用 grid 布局
            let mut clicked_id: Option<Uuid> = None;
            egui::Grid::new("card_grid")
                .spacing(Vec2::new(spacing, spacing))
                .show(ui, |ui| {
                    for (k, &idx) in filtered.iter().enumerate() {
                        if k > 0 && k % columns == 0 {
                            ui.end_row();
                        }
                        let card = &self.cards[idx];
                        if self.draw_card(ui, card, card_width, card_height) {
                            clicked_id = Some(card.id);
                        }
                    }
                });
            if let Some(id) = clicked_id {
                self.open_card_id = Some(id);
            }
        });

        // ── 卡片详情弹窗 ──
        if let Some(card_id) = self.open_card_id {
            self.show_card_detail(ctx, card_id);
        }
    }

    /// 绘制一张卡片。返回 `true` 表示卡片被点击。
    fn draw_card(&self, ui: &mut egui::Ui, card: &CardElement, width: f32, height: f32) -> bool {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click());

        // 卡片背景和边框
        let painter = ui.painter_at(rect);

        // 阴影效果（底层略大的暗色矩形）
        painter.rect_filled(
            rect.translate(Vec2::new(2.0, 3.0)),
            10.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 20),
        );

        // 主卡片
        let bg_color = if response.hovered() {
            Color32::from_rgb(250, 250, 255)
        } else {
            Color32::WHITE
        };
        painter.rect_filled(rect, 10.0, bg_color);
        painter.rect_stroke(
            rect,
            10.0,
            Stroke::new(
                1.5,
                if response.hovered() {
                    Color32::from_rgb(58, 134, 255)
                } else {
                    Color32::from_rgb(220, 220, 220)
                },
            ),
        );

        // 卡片顶部色带（按学科着色）
        let accent_color = self.subject_color(card.subject);
        let _accent_rect = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + 6.0));
        // 顶部圆角需要特殊处理，简单起见用矩形
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(rect.min.x + 0.5, rect.min.y + 0.5),
                Pos2::new(rect.max.x - 0.5, rect.min.y + 6.0),
            ),
            0.0,
            accent_color,
        );
        // 左上角圆角处理：画一个小圆角
        painter.circle_filled(
            Pos2::new(rect.min.x + 10.0, rect.min.y + 10.0),
            9.0,
            accent_color,
        );
        painter.circle_filled(
            Pos2::new(rect.max.x - 10.0, rect.min.y + 10.0),
            9.0,
            accent_color,
        );

        // 卡片图标（大 emoji 作为视觉中心）
        let icon_pos = Pos2::new(rect.center().x, rect.min.y + 50.0);
        painter.text(
            icon_pos,
            egui::Align2::CENTER_CENTER,
            card.data.type_emoji(),
            egui::FontId::proportional(36.0),
            Color32::BLACK,
        );

        // 卡片标题
        painter.text(
            Pos2::new(rect.center().x, rect.min.y + 85.0),
            egui::Align2::CENTER_TOP,
            &card.title,
            egui::FontId::proportional(15.0),
            Color32::from_rgb(40, 40, 40),
        );

        // 卡片类型标签
        let type_label = format!("{} {}", card.data.type_emoji(), card.data.type_label());
        painter.text(
            Pos2::new(rect.center().x, rect.min.y + 110.0),
            egui::Align2::CENTER_TOP,
            type_label,
            egui::FontId::proportional(11.0),
            Color32::from_rgb(120, 120, 120),
        );

        // 底部：难度 + 学科
        let bottom_y = rect.max.y - 16.0;
        if let Some(diff) = card.difficulty {
            let stars = "⭐".repeat(diff as usize);
            painter.text(
                Pos2::new(rect.min.x + 12.0, bottom_y),
                egui::Align2::LEFT_BOTTOM,
                stars,
                egui::FontId::proportional(10.0),
                Color32::from_rgb(255, 180, 0),
            );
        }

        painter.text(
            Pos2::new(rect.max.x - 12.0, bottom_y),
            egui::Align2::RIGHT_BOTTOM,
            format!("{} {}", card.subject.emoji(), card.subject.label()),
            egui::FontId::proportional(10.0),
            Color32::from_rgb(100, 100, 100),
        );

        // 点击打开详情
        response.clicked()
    }

    /// 学科对应的主题色。
    fn subject_color(&self, subject: Subject) -> Color32 {
        match subject {
            Subject::Chinese => Color32::from_rgb(244, 67, 54),
            Subject::Math => Color32::from_rgb(33, 150, 243),
            Subject::English => Color32::from_rgb(76, 175, 80),
            Subject::Physics => Color32::from_rgb(255, 152, 0),
            Subject::Chemistry => Color32::from_rgb(156, 39, 176),
            Subject::Biology => Color32::from_rgb(0, 150, 136),
            Subject::History => Color32::from_rgb(121, 85, 72),
            Subject::Geography => Color32::from_rgb(0, 188, 212),
            Subject::Politics => Color32::from_rgb(233, 30, 99),
            Subject::Other => Color32::from_rgb(158, 158, 158),
        }
    }

    /// 显示卡片详情弹窗。
    fn show_card_detail(&mut self, ctx: &egui::Context, card_id: Uuid) {
        // 找到卡片
        let card = match self.cards.iter().find(|c| c.id == card_id) {
            Some(c) => c.clone(), // 克隆一份，避免借用冲突
            None => {
                self.open_card_id = None;
                return;
            }
        };

        let mut open = true;
        let title = format!("{}  {}", card.data.type_emoji(), card.title);

        egui::Window::new(title)
            .id(egui::Id::new(("card_detail", card_id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .default_height(500.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| match &card.data {
                SubjectCard::Quiz(quiz_data) => {
                    self.show_quiz_detail(ui, card_id, quiz_data);
                }
                SubjectCard::Experiment(exp_data) => {
                    self.show_experiment_detail(ui, card_id, exp_data);
                }
                SubjectCard::Homework(hw_data) => {
                    self.show_homework_detail(ui, hw_data);
                }
                SubjectCard::Video(video_data) => {
                    self.show_video_detail(ui, video_data);
                }
                SubjectCard::DrawingBoard(board_data) => {
                    self.show_drawing_detail(ui, board_data);
                }
            });

        if !open {
            self.open_card_id = None;
        }
    }

    /// 显示题库详情。
    fn show_quiz_detail(
        &mut self,
        ui: &mut egui::Ui,
        card_id: Uuid,
        quiz_data: &crate::workshop::quiz::QuizCardData,
    ) {
        // 描述
        ui.label(&quiz_data.description);
        ui.add_space(8.0);

        // 进度条
        let progress = quiz_data.progress();
        let bar_width = ui.available_width();
        let bar_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(bar_width, 8.0));
        ui.painter()
            .rect_filled(bar_rect, 4.0, Color32::from_rgb(230, 230, 230));
        ui.painter().rect_filled(
            Rect::from_min_size(bar_rect.min, Vec2::new(bar_width * progress, 8.0)),
            4.0,
            Color32::from_rgb(76, 175, 80),
        );
        ui.add_space(12.0);

        ui.label(
            egui::RichText::new(format!(
                "进度：{}/{} 题  |  正确率：{:.0}%",
                quiz_data.completed_indices.len(),
                quiz_data.total_count(),
                quiz_data.accuracy() * 100.0
            ))
            .small()
            .color(Color32::from_rgb(100, 100, 100)),
        );

        ui.separator();
        ui.add_space(8.0);

        // 获取或创建答题状态
        let state = self.quiz_states.entry(card_id).or_insert_with(|| {
            let mut s = crate::workshop::quiz::QuizState::default();
            s.current_index = 0;
            s
        });

        // 确保 current_index 在范围内
        if state.current_index >= quiz_data.questions.len() {
            state.current_index = quiz_data.questions.len().saturating_sub(0);
        }

        if quiz_data.questions.is_empty() {
            ui.label("暂无题目");
            return;
        }

        // 题目导航
        ui.horizontal(|ui| {
            if ui
                .add_enabled(state.current_index > 0, egui::Button::new("← 上一题"))
                .clicked()
            {
                state.current_index -= 1;
                state.submitted = false;
                state.user_answers.clear();
                state.show_analysis = false;
            }

            ui.label(
                egui::RichText::new(format!(
                    "第 {}/{} 题",
                    state.current_index + 1,
                    quiz_data.questions.len()
                ))
                .strong(),
            );

            if ui
                .add_enabled(
                    state.current_index + 1 < quiz_data.questions.len(),
                    egui::Button::new("下一题 →"),
                )
                .clicked()
            {
                state.current_index += 1;
                state.submitted = false;
                state.user_answers.clear();
                state.show_analysis = false;
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // 绘制当前题目
        if let Some(question) = quiz_data.questions.get(state.current_index) {
            draw_question(ui, question, state);
        }
    }

    /// 显示实验详情。
    fn show_experiment_detail(
        &mut self,
        ui: &mut egui::Ui,
        card_id: Uuid,
        exp_data: &crate::workshop::experiment::ExperimentCardData,
    ) {
        ui.label(&exp_data.description);
        ui.add_space(8.0);

        ui.label(
            egui::RichText::new(format!(
                "难度：{}  |  步骤：{} 步",
                "⭐".repeat(exp_data.difficulty as usize),
                exp_data.step_count
            ))
            .small()
            .color(Color32::from_rgb(100, 100, 100)),
        );

        ui.separator();
        ui.add_space(12.0);

        // 不同类型的实验显示不同的内容
        match exp_data.exp_type {
            crate::workshop::experiment::ExperimentType::Circuit => {
                // 电路实验：显示可交互的电路图
                ui.heading("⚡ 串联电路实验");
                ui.add_space(8.0);

                let state = self
                    .circuit_states
                    .entry(card_id)
                    .or_insert_with(SimpleCircuitState::default);

                // 电路图区域
                let (rect, _response) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 200.0),
                    egui::Sense::click(),
                );
                let painter = ui.painter_at(rect);

                // 绘制电路（使用简化版：直接画在 egui 上）
                Self::draw_simple_circuit(&painter, rect, state);

                ui.add_space(8.0);

                // 控制按钮
                ui.horizontal(|ui| {
                    let btn_text = if state.switch_closed {
                        "🔴 断开开关"
                    } else {
                        "🟢 闭合开关"
                    };
                    if ui.button(btn_text).clicked() {
                        state.toggle_switch();
                    }

                    ui.separator();

                    ui.label(format!("电压：{:.1} V", state.voltage));
                    ui.label(format!("电阻：{:.1} Ω", state.total_resistance));
                    ui.label(format!("电流：{:.2} A", state.current));
                });

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("💡 提示：点击开关，观察灯泡的变化和电流的大小。")
                        .small()
                        .color(Color32::from_rgb(120, 120, 120)),
                );
            }
            _ => {
                ui.label(
                    egui::RichText::new("🔬 实验模拟器开发中...")
                        .color(Color32::from_rgb(150, 150, 150)),
                );
                ui.add_space(4.0);
                ui.label(egui::RichText::new("更多实验类型即将上线！").small());
            }
        }
    }

    /// 绘制简单的串联电路示意图（直接在 egui painter 上画）。
    fn draw_simple_circuit(painter: &egui::Painter, rect: Rect, state: &SimpleCircuitState) {
        let stroke_color = if state.bulb_lit {
            Color32::from_rgb(255, 160, 0)
        } else {
            Color32::from_rgb(60, 60, 60)
        };
        let stroke = Stroke::new(2.5, stroke_color);

        let left = rect.left() + 30.0;
        let right = rect.right() - 30.0;
        let top = rect.top() + 40.0;
        let bottom = rect.bottom() - 40.0;

        // 矩形回路
        painter.line_segment([Pos2::new(left, top), Pos2::new(right, top)], stroke);
        painter.line_segment([Pos2::new(left, bottom), Pos2::new(right, bottom)], stroke);
        painter.line_segment([Pos2::new(left, top), Pos2::new(left, bottom)], stroke);
        painter.line_segment([Pos2::new(right, top), Pos2::new(right, bottom)], stroke);

        // 元件位置（在顶部导线上均匀分布）
        let comp_count = 4;
        let spacing = (right - left) / (comp_count + 1) as f32;

        // 1. 电池
        let bat_x = left + spacing;
        painter.line_segment(
            [Pos2::new(bat_x, top - 8.0), Pos2::new(bat_x, top + 8.0)],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(bat_x - 6.0, top - 5.0),
                Pos2::new(bat_x - 6.0, top + 5.0),
            ],
            stroke,
        );

        // 2. 开关
        let sw_x = left + spacing * 2.0;
        if state.switch_closed {
            painter.line_segment(
                [Pos2::new(sw_x - 10.0, top), Pos2::new(sw_x + 10.0, top)],
                stroke,
            );
        } else {
            painter.line_segment(
                [
                    Pos2::new(sw_x - 10.0, top),
                    Pos2::new(sw_x + 8.0, top - 12.0),
                ],
                stroke,
            );
        }
        painter.circle_filled(Pos2::new(sw_x - 10.0, top), 3.0, stroke_color);
        painter.circle_filled(Pos2::new(sw_x + 10.0, top), 3.0, stroke_color);

        // 3. 灯泡
        let bulb_x = left + spacing * 3.0;
        let bulb_r = 12.0;
        let bulb_center = Pos2::new(bulb_x, top);
        if state.bulb_lit {
            painter.circle_filled(bulb_center, bulb_r, Color32::from_rgb(255, 230, 100));
            // 发光光晕
            painter.circle_stroke(
                bulb_center,
                bulb_r + 6.0,
                Stroke::new(3.0, Color32::from_rgba_unmultiplied(255, 200, 0, 100)),
            );
        } else {
            painter.circle_filled(bulb_center, bulb_r, Color32::WHITE);
        }
        painter.circle_stroke(bulb_center, bulb_r, stroke);
        // 灯丝十字
        painter.line_segment(
            [
                Pos2::new(bulb_x - bulb_r * 0.5, top),
                Pos2::new(bulb_x + bulb_r * 0.5, top),
            ],
            stroke,
        );
        painter.line_segment(
            [
                Pos2::new(bulb_x, top - bulb_r * 0.5),
                Pos2::new(bulb_x, top + bulb_r * 0.5),
            ],
            stroke,
        );

        // 4. 电阻
        let res_x = left + spacing * 4.0;
        let res_w = 28.0;
        let res_h = 8.0;
        let teeth = 4;
        let tooth_w = res_w / teeth as f32;
        for i in 0..teeth {
            let x_start = res_x - res_w / 2.0 + i as f32 * tooth_w;
            painter.line_segment(
                [
                    Pos2::new(x_start, top),
                    Pos2::new(x_start + tooth_w * 0.5, top - res_h),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(x_start + tooth_w * 0.5, top - res_h),
                    Pos2::new(x_start + tooth_w, top),
                ],
                stroke,
            );
        }
    }

    /// 显示作业详情。
    fn show_homework_detail(
        &self,
        ui: &mut egui::Ui,
        hw_data: &crate::workshop::cards::HomeworkCardData,
    ) {
        ui.heading("📋 作业详情");
        ui.add_space(8.0);
        ui.label(&hw_data.description);
        ui.add_space(8.0);
        ui.label(format!("题目数量：{}", hw_data.question_count));
        if let Some(deadline) = hw_data.deadline {
            ui.label(format!("截止日期：{}", deadline));
        }
        if hw_data.completed {
            ui.label(format!("得分：{:.1} / 100", hw_data.score.unwrap_or(0.0)));
        }
    }

    /// 显示视频详情。
    fn show_video_detail(
        &self,
        ui: &mut egui::Ui,
        video_data: &crate::workshop::cards::VideoCardData,
    ) {
        ui.heading("🎬 视频资源");
        ui.add_space(8.0);
        ui.label(&video_data.description);
        ui.add_space(8.0);

        if let Some(dur) = video_data.duration_sec {
            ui.label(format!("时长：{} 分 {} 秒", dur / 60, dur % 60));
        }
        ui.label(format!(
            "状态：{}",
            if video_data.downloaded {
                "已下载"
            } else {
                "未下载"
            }
        ));

        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("🎥 视频播放器开发中...").color(Color32::from_rgb(150, 150, 150)),
        );
    }

    /// 显示画板详情。
    fn show_drawing_detail(
        &self,
        ui: &mut egui::Ui,
        board_data: &crate::workshop::cards::DrawingBoardData,
    ) {
        ui.heading("🎨 教学画板");
        ui.add_space(8.0);
        ui.label(&board_data.description);
        ui.add_space(8.0);
        ui.label(format!("背景：{:?}", board_data.background));

        ui.add_space(16.0);
        if ui.button("在白板中打开").clicked() {
            // 可以触发在主白板中新建一页
        }
    }
}
