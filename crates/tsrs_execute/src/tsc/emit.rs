use std::cell::Cell;

use tsrs_compiler::{Program, get_diagnostics_of_any_program, sort_and_deduplicate_diagnostics};
use tsrs_core::P;
use tsrs_tsoptions::ParsedCommandLine;

use super::{
    CompileAndEmitResult, CompileTimes, DiagnosticReporter, DiagnosticsReporter, ExitStatus,
    Statistics, System, statistics_from_program,
};

pub struct EmitInput<'a> {
    pub sys: &'a dyn System,
    pub program: &'static Program,
    pub config: P<ParsedCommandLine>,
    pub report_diagnostic: &'a DiagnosticReporter<'a>,
    pub report_error_summary: &'a DiagnosticsReporter<'a>,
    pub compile_times: CompileTimes,
    // Go `EmitInput.ProgramLike` when it is an `*incremental.Program`.
    pub incremental: Option<P<tsrs_incremental::Program>>,
    // Go `EmitInput.Writer` (nil here means `sys.Writer()`), `EmitInput.WriteFile` and `EmitInput.Testing`.
    pub writer: Option<&'a (dyn Fn(&str) + 'a)>,
    pub write_file: Option<tsrs_compiler::WriteFile<'a>>,
    pub testing: Option<&'a dyn super::CommandLineTesting>,
    pub testing_m_times_cache: Option<super::MTimesCache>,
}

impl EmitInput<'_> {
    pub fn write(&self, text: &str) {
        match self.writer {
            Some(w) => w(text),
            None => self.sys.write(text),
        }
    }
}

pub fn emit_and_report_statistics(input: &EmitInput) -> (CompileAndEmitResult, Option<Statistics>) {
    let mut statistics = None;
    let mut result = emit_files_and_report_errors(input);
    if result.status != ExitStatus::Success {
        // compile exited early
        return (result, None);
    }
    let options = input.config.compiler_options().unwrap();
    if options.diagnostics_enabled() {
        result.times.total_time = input.sys.since_start();
        let stats = tsrs_core::phases::time(
            input.program.options().diagnostics_enabled(),
            "Statistics",
            || statistics_from_program(input, &result.times),
        );
        stats.report(&|t: &str| input.write(t), input.testing);
        if tsrs_compiler::Checker::census_enabled() {
            input.program.for_each_checker_parallel(|_, c| c.census_collect());
            tsrs_compiler::Checker::census_write_report();
        }
        statistics = Some(stats);
    }

    if result.emit_skipped && !result.diagnostics.is_empty() {
        result.status = ExitStatus::DiagnosticsPresent_OutputsSkipped;
    } else if !result.diagnostics.is_empty() {
        result.status = ExitStatus::DiagnosticsPresent_OutputsGenerated;
    }
    (result, statistics)
}

pub fn emit_files_and_report_errors(input: &EmitInput) -> CompileAndEmitResult {
    if let Some(incremental) = input.incremental {
        return emit_files_and_report_errors_incremental(input, incremental);
    }
    let mut times = input.compile_times.clone();
    let bind_time = Cell::new(times.bind_time);
    let check_time = Cell::new(times.check_time);
    let program = input.program;

    let ctx = tsrs_compiler::Context::default();
    let mut all_diagnostics = get_diagnostics_of_any_program(
        &ctx,
        program,
        None,
        false,
        &mut |ctx, file| {
            // Options diagnostics include global diagnostics (even though we collect them separately),
            // and global diagnostics create checkers, which then bind all of the files. Do this binding
            // early so we can track the time.
            let bind_start = super::DiagnosticTimer::new(
                input.sys,
                input.program.options().diagnostics_enabled(),
            );
            let diags = program.get_bind_diagnostics(ctx, file);
            bind_time.set(bind_start.elapsed());
            diags
        },
        &mut |ctx, file| {
            let check_start = super::DiagnosticTimer::new(
                input.sys,
                input.program.options().diagnostics_enabled(),
            );
            let diags = program.get_semantic_diagnostics(ctx, file);
            check_time.set(check_start.elapsed());
            diags
        },
    );
    times.bind_time = bind_time.get();
    times.check_time = check_time.get();

    // emit.go:115
    let mut emit_result = tsrs_compiler::EmitResult { emit_skipped: true, ..Default::default() };
    if !program.options().list_files_only.is_true() {
        let emit_start =
            super::DiagnosticTimer::new(input.sys, input.program.options().diagnostics_enabled());
        emit_result =
            tsrs_core::phases::time(input.program.options().diagnostics_enabled(), "Emit", || {
                program.emit(
                    &ctx,
                    &tsrs_compiler::EmitOptions {
                        write_file: input.write_file,
                        ..Default::default()
                    },
                )
            });
        times.emit_time += emit_start.elapsed();
    }
    let emit_skipped = emit_result.emit_skipped;
    all_diagnostics.extend(emit_result.diagnostics.iter().copied());
    if let Some(testing) = input.testing {
        testing.on_emitted_files(Some(&emit_result), input.testing_m_times_cache.as_ref());
    }

    let all_diagnostics = tsrs_core::phases::time(
        input.program.options().diagnostics_enabled(),
        "Diagnostics: sort",
        || sort_and_deduplicate_diagnostics(&all_diagnostics),
    );
    tsrs_core::phases::time(
        input.program.options().diagnostics_enabled(),
        "Diagnostics: report",
        || {
            for &diagnostic in &all_diagnostics {
                (input.report_diagnostic)(diagnostic);
            }
        },
    );

    tsrs_core::phases::time(input.program.options().diagnostics_enabled(), "List files", || {
        list_files(input, &emit_result);
    });

    tsrs_core::phases::time(input.program.options().diagnostics_enabled(), "Error summary", || {
        (input.report_error_summary)(&all_diagnostics);
    });
    let emitted_files = emit_result.emitted_files.clone();
    CompileAndEmitResult {
        diagnostics: all_diagnostics,
        emit_skipped,
        emitted_files,
        status: ExitStatus::Success,
        times,
    }
}

// emit.go:142
fn list_files(input: &EmitInput, emit_result: &tsrs_compiler::EmitResult) {
    if let Some(testing) = input.testing {
        testing.on_list_files_start(&|t: &str| input.write(t));
    }
    list_files_worker(input, emit_result);
    if let Some(testing) = input.testing {
        testing.on_list_files_end(&|t: &str| input.write(t));
    }
}

fn list_files_worker(input: &EmitInput, emit_result: &tsrs_compiler::EmitResult) {
    let options = input.program.options();
    if options.list_emitted_files.is_true() {
        let mut out = String::new();
        for file in &emit_result.emitted_files {
            out.push_str("TSFILE: ");
            out.push_str(&tsrs_core::tspath::get_normalized_absolute_path(
                file,
                input.program.get_current_directory(),
            ));
            out.push('\n');
        }
        input.write(&out);
    }
    if options.explain_files.is_true() {
        let mut out = Vec::new();
        input.program.explain_files(&mut out);
        input.write(&tsrs_core::utf8::from_utf8_lossy(&out));
    } else if options.list_files.is_true() || options.list_files_only.is_true() {
        let mut out = String::new();
        out.extend(
            input.program.get_source_files().iter().flat_map(|file| [file.file_name(), "\n"]),
        );
        input.write(&out);
    }
}

// emit.go:72 EmitFilesAndReportErrors with an incremental program as the ProgramLike.
fn emit_files_and_report_errors_incremental(
    input: &EmitInput,
    program_like: P<tsrs_incremental::Program>,
) -> CompileAndEmitResult {
    let program_like: &'static tsrs_incremental::Program = program_like.get();
    use tsrs_incremental::emit::{
        EmitOptions, EmitResult, ProgramLike,
        get_diagnostics_of_any_program as get_diagnostics_of_any_program_like,
    };
    let mut times = input.compile_times.clone();
    let bind_time = Cell::new(times.bind_time);
    let check_time = Cell::new(times.check_time);
    let emit_time = Cell::new(times.emit_time);

    let ctx = tsrs_compiler::Context::default();
    let mut all_diagnostics = get_diagnostics_of_any_program_like(
        &ctx,
        &program_like,
        None,
        false,
        &mut |ctx, file| {
            // Options diagnostics include global diagnostics (even though we collect them separately),
            // and global diagnostics create checkers, which then bind all of the files. Do this binding
            // early so we can track the time.
            let bind_start = super::DiagnosticTimer::new(
                input.sys,
                input.program.options().diagnostics_enabled(),
            );
            let diags = program_like.get_bind_diagnostics(ctx, file);
            bind_time.set(bind_start.elapsed());
            diags
        },
        &mut |ctx, file| {
            let check_start = super::DiagnosticTimer::new(
                input.sys,
                input.program.options().diagnostics_enabled(),
            );
            let diags = program_like.get_semantic_diagnostics(ctx, file);
            check_time.set(check_start.elapsed());
            let nested_emit_time = program_like.take_nested_emit_time();
            if nested_emit_time > check_time.get() {
                check_time.set(std::time::Duration::ZERO);
            } else {
                check_time.set(check_time.get().checked_sub(nested_emit_time).unwrap());
            }
            emit_time.set(emit_time.get() + nested_emit_time);
            diags
        },
    );
    times.bind_time = bind_time.get();
    times.check_time = check_time.get();
    times.emit_time = emit_time.get();

    let mut emit_result = Some(EmitResult { emit_skipped: true, ..Default::default() });
    if !program_like.options().list_files_only.is_true() {
        let emit_start =
            super::DiagnosticTimer::new(input.sys, input.program.options().diagnostics_enabled());
        emit_result = program_like
            .emit(&ctx, EmitOptions { write_file: input.write_file, ..Default::default() });
        times.emit_time += emit_start.elapsed();
    }
    if let Some(emit_result) = &emit_result {
        all_diagnostics.extend(emit_result.diagnostics.iter().copied());
    }
    if let Some(testing) = input.testing {
        testing.on_emitted_files(emit_result.as_ref(), input.testing_m_times_cache.as_ref());
    }

    let all_diagnostics = sort_and_deduplicate_diagnostics(&all_diagnostics);
    for &diagnostic in &all_diagnostics {
        (input.report_diagnostic)(diagnostic);
    }

    list_files(input, &emit_result.clone().unwrap_or_default());

    (input.report_error_summary)(&all_diagnostics);
    // Go reads EmitResult.EmitSkipped through a nil result here only when the incremental program was cancelled.
    let emitted_files = emit_result.as_ref().map(|r| r.emitted_files.clone()).unwrap_or_default();
    let emit_skipped = emit_result.is_some_and(|r| r.emit_skipped);
    CompileAndEmitResult {
        diagnostics: all_diagnostics,
        emit_skipped,
        emitted_files,
        status: ExitStatus::Success,
        times,
    }
}
