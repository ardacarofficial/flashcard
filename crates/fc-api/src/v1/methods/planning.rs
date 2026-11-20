//! Smart planning (v0.6): exam dates, goals, FSRS optimization, workload estimates.

use crate::v1::common::Empty;
use crate::v1::planning::*;

methods! {
    // ---- planning: exam dates ----
    /// Exams with their countdown.
    PlanningExamsList = "planning.exams.list", Query, "planning", "planning:read", Public, undo: false, ListExams => ExamList;
    /// Adds an exam (exam-date mode for a deck and/or a query).
    PlanningExamsCreate = "planning.exams.create", Command, "planning", "planning:write", Public, undo: true, CreateExam => ExamDto;
    /// Edits an exam.
    PlanningExamsUpdate = "planning.exams.update", Command, "planning", "planning:write", Public, undo: true, UpdateExam => ExamDto;
    /// Removes an exam.
    PlanningExamsDelete = "planning.exams.delete", Command, "planning", "planning:write", Public, undo: true, ExamRef => Empty;
    /// Internal inverse of delete.
    PlanningExamsRestore = "planning.exams.restore", Command, "planning", "planning:write", Internal, undo: true, RestoreExam => ExamDto;

    // ---- goals ----
    /// Goals with progress and pace forecast.
    GoalsList = "goals.list", Query, "goals", "goals:read", Public, undo: false, ListGoals => GoalList;
    /// Sets a goal.
    GoalsCreate = "goals.create", Command, "goals", "goals:write", Public, undo: true, CreateGoal => GoalDto;
    /// Edits a goal.
    GoalsUpdate = "goals.update", Command, "goals", "goals:write", Public, undo: true, UpdateGoal => GoalDto;
    /// Removes a goal.
    GoalsDelete = "goals.delete", Command, "goals", "goals:write", Public, undo: true, GoalRef => Empty;
    /// Internal inverse of delete.
    GoalsRestore = "goals.restore", Command, "goals", "goals:write", Internal, undo: true, RestoreGoal => GoalDto;

    // ---- optimizer ----
    /// Starts FSRS optimization of a preset as a cancellable background job.
    OptimizerRun = "optimizer.run", Command, "optimizer", "decks:write", Public, undo: false, RunOptimizer => OptimizerStarted;
    /// Estimated daily reviews for several desired retentions.
    OptimizerWorkload = "optimizer.workload", Query, "optimizer", "decks:read", Public, undo: false, WorkloadRequest => WorkloadEstimate;
}
