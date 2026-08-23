//! UI module — all view rendering for the desktop app.
//!
//! | Module      | View                         | Description                              |
//! |-------------|------------------------------|------------------------------------------|
//! | `sidebar`   | Left navigation              | 备课 / 上课 / 批改 / 设置 buttons         |
//! | `prepare`   | Lesson preparation           | Canvas, element toolbar, slide list      |
//! | `teach`     | Teaching / presentation      | Fullscreen canvas, sync, quiz, stats     |
//! | `grade`     | Grading                      | Submission list, viewer, grading panel   |
//! | `settings`  | Settings                     | Backend URL, login, plugins, about       |

pub mod sidebar;
pub mod prepare;
pub mod teach;
pub mod grade;
pub mod settings;
