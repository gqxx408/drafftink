//! WasmApp — the student-facing homework editor application.
//!
//! Implements [`eframe::App`] so it can be driven by the eframe web runner
//! on WASM. The struct holds all UI state and delegates crypto / drftx /
//! business logic to [`drafftink_core`].

use eframe;
use egui;
use uuid::Uuid;

use crate::{browser, crypto, offline, ui};

// ════════════════════════════════════════════════════════════════════════════
//  AnswerPayload — serializable student answer (text + strokes)
// ════════════════════════════════════════════════════════════════════════════

/// Student answer payload containing text and drawing strokes.
///
/// Serialized to JSON bytes and used as `answer_data` in
/// [`drafftink_core::drftx::ExerciseSnapshot`].
#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
pub struct AnswerPayload {
    /// Free-text answer.
    pub text: String,
    /// Drawing strokes, each stroke is a list of `[x, y]` points.
    pub strokes: Vec<Vec<[f32; 2]>>,
}

impl AnswerPayload {
    /// Serialize to JSON bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Deserialize from JSON bytes.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }

    /// Build from egui stroke data.
    pub fn from_egui(text: &str, strokes: &[Vec<egui::Pos2>]) -> Self {
        Self {
            text: text.to_string(),
            strokes: strokes
                .iter()
                .map(|s| s.iter().map(|p| [p.x, p.y]).collect())
                .collect(),
        }
    }

    /// Convert strokes back to egui `Pos2` vectors.
    pub fn to_egui_strokes(&self) -> Vec<Vec<egui::Pos2>> {
        self.strokes
            .iter()
            .map(|s| s.iter().map(|p| egui::pos2(p[0], p[1])).collect())
            .collect()
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  WasmApp
// ════════════════════════════════════════════════════════════════════════════

/// The student-facing homework editor app.
pub struct WasmApp {
    // ── Required fields (per spec) ──
    /// Homework ID, loaded from URL param `?hw=xxx`.
    pub homework_id: Option<Uuid>,
    /// Student's answer data (serialized [`AnswerPayload`]).
    pub answer_data: Vec<u8>,
    /// Whether the draft has been saved.
    pub draft_saved: bool,
    /// Network status (online / offline).
    pub online: bool,
    /// Sync status message shown in the status bar.
    pub sync_status: String,
    /// Ed25519 private key (loaded from LocalStorage on WASM).
    pub student_key: Option<[u8; 32]>,

    // ── Additional state ──
    /// Student ID derived from the public key.
    pub(crate) student_id: Option<Uuid>,
    /// Text being edited in the answer area.
    pub(crate) answer_text: String,
    /// Completed drawing strokes.
    pub(crate) strokes: Vec<Vec<egui::Pos2>>,
    /// Stroke currently being drawn.
    pub(crate) current_stroke: Vec<egui::Pos2>,
    /// Submit status message.
    pub(crate) submit_status: String,
    /// Last auto-save time (seconds since app start).
    last_auto_save: f64,
    /// Last online-status check time.
    last_sync_check: f64,
    /// Last successful sync time.
    pub(crate) last_sync_time: Option<f64>,
    /// Whether the QR / parent-scan panel is visible.
    pub(crate) show_qr: bool,
}

impl WasmApp {
    /// Create a new `WasmApp`.
    ///
    /// Parses the homework ID from the URL, loads the student keypair
    /// from LocalStorage, and restores any saved draft.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Parse homework ID from URL
        let homework_id = browser::get_url_param("hw").and_then(|s| Uuid::parse_str(&s).ok());

        // Generate or load keypair
        let (student_key, student_id) = match crypto::generate_or_load_keypair() {
            Ok((sk, pk)) => {
                let id = derive_student_id(&pk);
                (Some(sk), Some(id))
            }
            Err(e) => {
                log::error!("keypair generation failed: {e:?}");
                (None, None)
            }
        };

        // Load draft if homework_id is set
        let (answer_text, answer_data, strokes) = homework_id
            .and_then(offline::load_draft)
            .and_then(|data| AnswerPayload::from_bytes(&data))
            .map(|p| {
                let data = p.to_bytes();
                let egui_strokes = p.to_egui_strokes();
                (p.text, data, egui_strokes)
            })
            .unwrap_or_default();

        Self {
            homework_id,
            answer_data,
            draft_saved: false,
            online: browser::is_online(),
            sync_status: String::new(),
            student_key,
            student_id,
            answer_text,
            strokes,
            current_stroke: Vec::new(),
            submit_status: String::new(),
            last_auto_save: 0.0,
            last_sync_check: 0.0,
            last_sync_time: None,
            show_qr: false,
        }
    }

    /// Save the current answer as a draft to LocalStorage.
    pub(crate) fn save_draft(&mut self) {
        let Some(hw_id) = self.homework_id else {
            self.sync_status = "No homework ID".to_string();
            return;
        };
        let payload = AnswerPayload::from_egui(&self.answer_text, &self.strokes);
        let data = payload.to_bytes();
        offline::save_draft(hw_id, &data);
        self.answer_data = data;
        self.draft_saved = true;
        self.sync_status = "Draft saved".to_string();
    }

    /// Auto-save (called every 30 seconds by [`update`]).
    fn auto_save(&mut self) {
        if !self.answer_text.is_empty() || !self.strokes.is_empty() {
            self.save_draft();
        }
    }

    /// Submit the homework as a signed drftx file.
    pub(crate) fn submit(&mut self) {
        let Some(hw_id) = self.homework_id else {
            self.submit_status = "No homework loaded".to_string();
            return;
        };
        let Some(sk) = self.student_key else {
            self.submit_status = "No student key".to_string();
            return;
        };
        let Some(stu_id) = self.student_id else {
            self.submit_status = "No student ID".to_string();
            return;
        };

        // Build answer data
        let payload = AnswerPayload::from_egui(&self.answer_text, &self.strokes);
        let answer_data = payload.to_bytes();
        self.answer_data = answer_data.clone();

        // Create snapshot + signature + drftx (all via drafftink-core)
        use drafftink_core::drftx::{sign_snapshot, DrftxFile, ExerciseSnapshot};

        let snapshot = ExerciseSnapshot::new(hw_id, stu_id, answer_data);
        let signature = match sign_snapshot(&snapshot, &sk) {
            Ok(sig) => sig,
            Err(e) => {
                self.submit_status = format!("Sign error: {e}");
                return;
            }
        };
        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: None,
            recording: None,
        };
        let drftx_bytes = match file.to_bytes() {
            Ok(b) => b,
            Err(e) => {
                self.submit_status = format!("Serialize error: {e}");
                return;
            }
        };

        // Store as pending submission (for offline retry)
        offline::add_pending_submission(hw_id, drftx_bytes.clone());

        // Attempt immediate submission
        if let Err(e) = browser::submit_homework("/api/submit", drftx_bytes) {
            self.submit_status = format!("Submit error: {e}");
        } else {
            self.submit_status = "Submitting...".to_string();
            self.last_sync_time = Some(0.0); // will be updated by try_resubmit
        }

        // Try to resubmit any pending items
        offline::try_resubmit();

        self.draft_saved = false;
    }

    /// Build a text representation of the homework status for parent QR scanning.
    pub(crate) fn qr_text(&self) -> String {
        let hw = self
            .homework_id
            .map(|u| u.to_string())
            .unwrap_or_else(|| "N/A".to_string());
        let stu = self
            .student_id
            .map(|u| u.to_string())
            .unwrap_or_else(|| "N/A".to_string());
        let status = if self.submit_status.is_empty() {
            "Not submitted"
        } else {
            &self.submit_status
        };
        let online = if self.online { "online" } else { "offline" };
        format!("DRAFFTINK\nHW:{hw}\nStudent:{stu}\nStatus:{status}\nNet:{online}")
    }
}

impl eframe::App for WasmApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);

        // ── Auto-save every 30 seconds ──
        if now - self.last_auto_save > 30.0 {
            self.auto_save();
            self.last_auto_save = now;
        }

        // ── Check online status every 5 seconds ──
        if now - self.last_sync_check > 5.0 {
            let was_online = self.online;
            self.online = browser::is_online();
            if self.online && !was_online {
                offline::try_resubmit();
                self.sync_status = "Back online — syncing".to_string();
                self.last_sync_time = Some(now);
            } else if !self.online && was_online {
                self.sync_status = "Offline — draft will be saved locally".to_string();
            }
            self.last_sync_check = now;
        }

        // ── Status bar (bottom) ──
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui::status::render(ui, self, now);
        });

        // ── Editor (central panel) ──
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::editor::render(ui, self);
        });
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Helpers
// ════════════════════════════════════════════════════════════════════════════

/// Derive a deterministic student ID from the first 16 bytes of the public key.
fn derive_student_id(public_key: &[u8; 32]) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&public_key[..16]);
    Uuid::from_bytes(bytes)
}

// ════════════════════════════════════════════════════════════════════════════
//  Unit tests (run on native)
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_answer_payload_roundtrip() {
        let payload = AnswerPayload {
            text: "My answer".to_string(),
            strokes: vec![vec![[1.0, 2.0], [3.0, 4.0]], vec![[5.0, 6.0]]],
        };
        let bytes = payload.to_bytes();
        let restored = AnswerPayload::from_bytes(&bytes).unwrap();
        assert_eq!(restored.text, payload.text);
        assert_eq!(restored.strokes, payload.strokes);
    }

    #[test]
    fn test_answer_payload_empty() {
        let payload = AnswerPayload::default();
        let bytes = payload.to_bytes();
        let restored = AnswerPayload::from_bytes(&bytes).unwrap();
        assert!(restored.text.is_empty());
        assert!(restored.strokes.is_empty());
    }

    #[test]
    fn test_answer_payload_from_invalid_bytes() {
        assert!(AnswerPayload::from_bytes(b"not json").is_none());
    }

    #[test]
    fn test_answer_payload_egui_conversion() {
        let strokes = vec![vec![egui::pos2(1.0, 2.0), egui::pos2(3.0, 4.0)]];
        let payload = AnswerPayload::from_egui("hello", &strokes);
        assert_eq!(payload.text, "hello");
        assert_eq!(payload.strokes, vec![vec![[1.0, 2.0], [3.0, 4.0]]]);

        let back = payload.to_egui_strokes();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].len(), 2);
        assert_eq!(back[0][0], egui::pos2(1.0, 2.0));
    }

    #[test]
    fn test_derive_student_id_deterministic() {
        let pk = [42u8; 32];
        let id1 = derive_student_id(&pk);
        let id2 = derive_student_id(&pk);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_derive_student_id_different_keys() {
        let pk1 = [1u8; 32];
        let pk2 = [2u8; 32];
        let id1 = derive_student_id(&pk1);
        let id2 = derive_student_id(&pk2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_qr_text_contains_homework_info() {
        let app = WasmApp {
            homework_id: Some(Uuid::nil()),
            answer_data: Vec::new(),
            draft_saved: false,
            online: true,
            sync_status: String::new(),
            student_key: None,
            student_id: Some(Uuid::nil()),
            answer_text: String::new(),
            strokes: Vec::new(),
            current_stroke: Vec::new(),
            submit_status: "Submitted".to_string(),
            last_auto_save: 0.0,
            last_sync_check: 0.0,
            last_sync_time: None,
            show_qr: false,
        };
        let qr = app.qr_text();
        assert!(qr.contains("DRAFFTINK"));
        assert!(qr.contains("HW:"));
        assert!(qr.contains("Student:"));
        assert!(qr.contains("Status:Submitted"));
        assert!(qr.contains("Net:online"));
    }

    #[test]
    fn test_qr_text_no_homework() {
        let app = WasmApp {
            homework_id: None,
            answer_data: Vec::new(),
            draft_saved: false,
            online: false,
            sync_status: String::new(),
            student_key: None,
            student_id: None,
            answer_text: String::new(),
            strokes: Vec::new(),
            current_stroke: Vec::new(),
            submit_status: String::new(),
            last_auto_save: 0.0,
            last_sync_check: 0.0,
            last_sync_time: None,
            show_qr: false,
        };
        let qr = app.qr_text();
        assert!(qr.contains("HW:N/A"));
        assert!(qr.contains("Status:Not submitted"));
        assert!(qr.contains("Net:offline"));
    }
}
