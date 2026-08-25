//! drftx 三层文件 ↔ ZXX（JY/T 1004）映射
//!
//! | drftx 数据 | ZXX 目标 |
//! |-----------|---------|
//! | 文件头元数据（课程/教材/学期） | ZXJX01 课程 |
//! | [`TeacherAnnotation`] 的 `score`（作业批改结果） | ZXXS0206 在校考试 |
//! | [`ExerciseSnapshot`] 的学生标识码（作答快照层） | ZXXS01 学生基本 |

use crate::drftx::TeacherAnnotation;
use crate::emgi::StudentBasic;
use crate::zxx::zxjx::Course;
use crate::zxx::zxxs::{ExamRecord, StudentProfile};
use crate::zxx::ZxxDataset;

/// 由课程元数据（通常来自备课/授课流程）构建 ZXJX01 课程记录。
#[allow(clippy::too_many_arguments)]
pub fn course_from_lesson(
    course_id: &str,
    course_code: &str,
    course_name: &str,
    course_type: &str,
    textbook_code: &str,
    textbook_name: &str,
    subject: &str,
    credit: &str,
    hours: &str,
    school_id: &str,
) -> Course {
    Course {
        course_id: course_id.to_string(),
        course_code: course_code.to_string(),
        course_name: course_name.to_string(),
        course_type: course_type.to_string(),
        textbook_code: textbook_code.to_string(),
        textbook_name: textbook_name.to_string(),
        subject: subject.to_string(),
        credit: credit.to_string(),
        hours: hours.to_string(),
        school_id: school_id.to_string(),
    }
}

/// 由作业批改批注（[`TeacherAnnotation`]）映射为 ZXXS0206 在校考试数据子类。
///
/// `score` 直接取自批注中的分数；未批改（`score = None`）时成绩项留空。
#[allow(clippy::too_many_arguments)]
pub fn exam_record_from_annotation(
    ann: &TeacherAnnotation,
    exam_id: &str,
    student_id: &str,
    school_id: &str,
    course_id: &str,
    academic_year: &str,
    term: &str,
    exam_method: &str,
    exam_date: &str,
    score_type: &str,
) -> ExamRecord {
    let score = ann.score.map(|s| format!("{s}")).unwrap_or_default();
    ExamRecord {
        exam_id: exam_id.to_string(),
        student_id: student_id.to_string(),
        school_id: school_id.to_string(),
        course_id: course_id.to_string(),
        academic_year: academic_year.to_string(),
        term: term.to_string(),
        exam_method: exam_method.to_string(),
        exam_date: exam_date.to_string(),
        score,
        score_type: score_type.to_string(),
    }
}

/// 由作答快照层关联出 ZXXS01 学生基本（需提供已「取用」的学生基础信息）。
pub fn student_profile_from_snapshot(student: &StudentBasic, student_no: &str) -> StudentProfile {
    StudentProfile::from_student(student.clone(), student_no)
}

/// 便捷组装：将课程、考试、学生三类记录合并为一个合规数据集。
pub fn dataset_from_components(
    course: Course,
    exam: ExamRecord,
    student: StudentProfile,
) -> ZxxDataset {
    let mut ds = ZxxDataset::new();
    ds.add(&course);
    ds.add(&exam);
    ds.add(&student);
    ds
}
