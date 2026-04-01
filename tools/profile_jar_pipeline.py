"""Profile JAR-processing stages with ``cProfile``.

Usage:
    uv run python tools/profile_jar_pipeline.py 225.jar
    uv run python tools/profile_jar_pipeline.py 225.jar --stages model-lift model-lower
    uv run python tools/profile_jar_pipeline.py path/to/jar-corpus --stages model-lift model-lower ^
        --summary-json output/profiles/common-libs/summary.json
"""

from __future__ import annotations

import argparse
import cProfile
import io
import json
import os
import pstats
import time
from collections import Counter
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path

from pytecode import ClassModel, ClassReader, ClassWriter, JarFile
from pytecode.info import ClassFile
from pytecode.jar import JarInfo

type ClassifiedEntries = tuple[list[JarInfo], list[JarInfo]]
type ParsedClasses = list[tuple[JarInfo, ClassReader]]
type LiftedModels = list[tuple[JarInfo, ClassModel]]
type LoweredClasses = list[tuple[JarInfo, ClassFile]]
type SerializedClasses = list[tuple[JarInfo, bytes]]

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CORPUS_OUTPUT_DIR = REPO_ROOT / "output" / "profiles" / "common-libs"
DEFAULT_CORPUS_STAGES = ("model-lift", "model-lower")

SORT_CHOICES = (
    "calls",
    "cumulative",
    "filename",
    "line",
    "name",
    "nfl",
    "pcalls",
    "stdname",
    "time",
)


def is_class_entry(filename: str) -> bool:
    """Return whether *filename* points at a non-directory ``.class`` entry."""
    return not filename.endswith(os.sep) and filename.endswith(".class")


def classify_entries(jar: JarFile) -> ClassifiedEntries:
    """Split loaded JAR entries into class files and pass-through resources."""
    class_entries: list[JarInfo] = []
    other_entries: list[JarInfo] = []
    for jar_info in jar.files.values():
        if is_class_entry(jar_info.filename):
            class_entries.append(jar_info)
        else:
            other_entries.append(jar_info)
    return class_entries, other_entries


def parse_class_entries(class_entries: Sequence[JarInfo]) -> ParsedClasses:
    """Parse raw ``.class`` bytes into ``ClassReader`` instances."""
    return [(jar_info, ClassReader.from_bytes(jar_info.bytes)) for jar_info in class_entries]


def lift_models(parsed_classes: Sequence[tuple[JarInfo, ClassReader]]) -> LiftedModels:
    """Lift parsed class files into mutable ``ClassModel`` instances."""
    return [(jar_info, ClassModel.from_classfile(reader.class_info)) for jar_info, reader in parsed_classes]


def lower_models(models: Sequence[tuple[JarInfo, ClassModel]]) -> LoweredClasses:
    """Lower mutable models back into raw ``ClassFile`` trees."""
    return [(jar_info, model.to_classfile()) for jar_info, model in models]


def serialize_classfiles(classfiles: Sequence[tuple[JarInfo, ClassFile]]) -> SerializedClasses:
    """Serialize raw ``ClassFile`` trees into ``.class`` bytes."""
    return [(jar_info, ClassWriter.write(classfile)) for jar_info, classfile in classfiles]


@dataclass
class ProfileInputs:
    """Lazy cache of shared setup state for isolated stage profiling."""

    jar_path: Path
    _archive: JarFile | None = None
    _classified: ClassifiedEntries | None = None
    _parsed: ParsedClasses | None = None

    def archive(self) -> JarFile:
        """Load the target JAR once for non-profiled setup work."""
        if self._archive is None:
            self._archive = JarFile(self.jar_path)
        return self._archive

    def classified(self) -> ClassifiedEntries:
        """Return cached class/resource entry separation."""
        if self._classified is None:
            self._classified = classify_entries(self.archive())
        return self._classified

    def parsed(self) -> ParsedClasses:
        """Return cached parsed class readers for later setup work."""
        if self._parsed is None:
            class_entries, _other_entries = self.classified()
            self._parsed = parse_class_entries(class_entries)
        return self._parsed


@dataclass(frozen=True)
class PreparedStage:
    """A fully configured profiling stage ready to execute."""

    name: str
    description: str
    workload: Callable[[], str]


@dataclass(frozen=True)
class StageReport:
    """Profile output for a single isolated stage."""

    index: int
    name: str
    description: str
    summary: str
    elapsed_seconds: float
    profile_path: Path | None
    stats_text: str


@dataclass(frozen=True)
class StageAggregate:
    """Aggregate elapsed-time statistics for one stage across many JARs."""

    count: int
    mean_seconds: float
    min_seconds: float
    max_seconds: float


@dataclass(frozen=True)
class JarProfileReport:
    """Profiling results for one JAR in a multi-JAR run."""

    jar_path: Path
    stage_reports: list[StageReport]


@dataclass(frozen=True)
class CorpusReport:
    """Aggregated profiling output for one or more JARs."""

    jars: list[JarProfileReport]
    stage_averages: dict[str, StageAggregate]


def prepare_jar_read(inputs: ProfileInputs) -> PreparedStage:
    """Profile loading ZIP entries into a ``JarFile``."""

    def workload() -> str:
        jar = JarFile(inputs.jar_path)
        class_entries, other_entries = classify_entries(jar)
        return f"entries={len(jar.files)} class_entries={len(class_entries)} other_entries={len(other_entries)}"

    return PreparedStage(
        name="jar-read",
        description="Read ZIP metadata and entry bytes into memory.",
        workload=workload,
    )


def prepare_jar_classify(inputs: ProfileInputs) -> PreparedStage:
    """Profile separating already-loaded entries into classes and resources."""
    jar = inputs.archive()

    def workload() -> str:
        class_entries, other_entries = classify_entries(jar)
        return f"entries={len(jar.files)} class_entries={len(class_entries)} other_entries={len(other_entries)}"

    return PreparedStage(
        name="jar-classify",
        description="Split loaded JAR entries into class and non-class groups.",
        workload=workload,
    )


def prepare_class_parse(inputs: ProfileInputs) -> PreparedStage:
    """Profile parsing raw class bytes into ``ClassReader`` objects."""
    class_entries, other_entries = inputs.classified()

    def workload() -> str:
        parsed_classes = parse_class_entries(class_entries)
        return f"parsed_classes={len(parsed_classes)} other_entries={len(other_entries)}"

    return PreparedStage(
        name="class-parse",
        description="Parse each class entry into a ClassReader.",
        workload=workload,
    )


def prepare_model_lift(inputs: ProfileInputs) -> PreparedStage:
    """Profile lifting parsed classes into editable models."""
    parsed_classes = inputs.parsed()

    def workload() -> str:
        models = lift_models(parsed_classes)
        method_count = sum(len(model.methods) for _jar_info, model in models)
        return f"models={len(models)} methods={method_count}"

    return PreparedStage(
        name="model-lift",
        description="Lift parsed ClassFile trees into ClassModel instances.",
        workload=workload,
    )


def prepare_model_lower(inputs: ProfileInputs) -> PreparedStage:
    """Profile lowering editable models back into raw classfile trees."""
    models = lift_models(inputs.parsed())

    def workload() -> str:
        classfiles = lower_models(models)
        total_pool_slots = sum(len(classfile.constant_pool) for _jar_info, classfile in classfiles)
        return f"classfiles={len(classfiles)} total_pool_slots={total_pool_slots}"

    return PreparedStage(
        name="model-lower",
        description="Lower ClassModel instances back into ClassFile trees.",
        workload=workload,
    )


def prepare_class_write(inputs: ProfileInputs) -> PreparedStage:
    """Profile serializing lowered classfile trees into ``bytes``."""
    classfiles = lower_models(lift_models(inputs.parsed()))

    def workload() -> str:
        serialized_classes = serialize_classfiles(classfiles)
        total_bytes = sum(len(class_bytes) for _jar_info, class_bytes in serialized_classes)
        return f"serialized_classes={len(serialized_classes)} total_bytes={total_bytes}"

    return PreparedStage(
        name="class-write",
        description="Serialize ClassFile trees into final class bytes.",
        workload=workload,
    )


STAGE_BUILDERS: dict[str, Callable[[ProfileInputs], PreparedStage]] = {
    "jar-read": prepare_jar_read,
    "jar-classify": prepare_jar_classify,
    "class-parse": prepare_class_parse,
    "model-lift": prepare_model_lift,
    "model-lower": prepare_model_lower,
    "class-write": prepare_class_write,
}


def positive_int(value: str) -> int:
    """Parse a command-line integer that must be greater than zero."""
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("value must be greater than zero")
    return parsed


def is_jar_file(path: Path) -> bool:
    """Return whether *path* is a concrete ``.jar`` file."""
    return path.is_file() and path.suffix.lower() == ".jar"


def is_single_jar_input(inputs: Sequence[Path]) -> bool:
    """Return whether *inputs* represent exactly one jar file input."""
    if len(inputs) != 1:
        return False
    return is_jar_file(inputs[0].expanduser())


def expand_jar_inputs(jar_inputs: Sequence[Path], *, recursive: bool = False) -> list[Path]:
    """Expand file and directory inputs into a stable list of JAR paths."""
    jar_paths: list[Path] = []
    seen: set[Path] = set()

    for jar_input in jar_inputs:
        candidate = jar_input.expanduser()
        if candidate.is_dir():
            matches = sorted(candidate.rglob("*.jar") if recursive else candidate.glob("*.jar"))
            if not matches:
                raise ValueError(f"No .jar files found under directory: {candidate}")
            for match in matches:
                resolved = match.resolve()
                if resolved not in seen:
                    jar_paths.append(resolved)
                    seen.add(resolved)
            continue

        if is_jar_file(candidate):
            resolved = candidate.resolve()
            if resolved not in seen:
                jar_paths.append(resolved)
                seen.add(resolved)
            continue

        raise ValueError(f"Input must be a .jar file or a directory containing .jar files: {candidate}")

    return jar_paths


def default_output_dir(jar_path: Path) -> Path:
    """Return the default output directory for per-stage ``.prof`` files."""
    return REPO_ROOT / "output" / "profiles" / jar_path.stem


def default_stage_names(inputs: Sequence[Path], explicit_stage_names: Sequence[str] | None) -> list[str]:
    """Return the effective stage list for the requested inputs."""
    if explicit_stage_names is not None:
        return list(explicit_stage_names)
    if is_single_jar_input(inputs):
        return list(STAGE_BUILDERS)
    return list(DEFAULT_CORPUS_STAGES)


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments for stage-isolated runtime profiling."""
    parser = argparse.ArgumentParser(
        description="Profile one jar or a directory of jars with cProfile.",
    )
    parser.add_argument(
        "inputs",
        nargs="+",
        type=Path,
        help="A .jar file, multiple .jar files, or directories containing .jar files.",
    )
    parser.add_argument(
        "--recursive",
        action="store_true",
        help="Search provided directories recursively for .jar files.",
    )
    parser.add_argument(
        "--stages",
        nargs="+",
        choices=tuple(STAGE_BUILDERS),
        default=None,
        help=(
            "Stages to profile. Defaults to all stages for a single jar, or "
            "model-lift/model-lower for multiple jars or directories."
        ),
    )
    parser.add_argument(
        "--sort",
        choices=SORT_CHOICES,
        default="cumulative",
        help="Sort key for the printed pstats summary.",
    )
    parser.add_argument(
        "--top",
        type=positive_int,
        default=30,
        help="Number of pstats rows to print per stage.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help=(
            "Directory where profile output should be written. Defaults to "
            "output/profiles/<jar-stem> for one jar, or output/profiles/common-libs for directories/multi-jar runs."
        ),
    )
    parser.add_argument(
        "--summary-json",
        type=Path,
        default=None,
        help="Optional JSON file to write with per-jar timings and aggregate stage averages.",
    )
    parser.add_argument(
        "--no-save",
        action="store_true",
        help="Print stats without writing .prof files.",
    )
    return parser.parse_args(argv)


def format_profile_stats(profile: cProfile.Profile, *, sort: str, top: int) -> str:
    """Format a ``pstats`` summary for a completed profile."""
    stream = io.StringIO()
    stats = pstats.Stats(profile, stream=stream)
    stats.sort_stats(sort)
    stats.print_stats(top)
    return stream.getvalue().rstrip()


def run_stage(
    index: int,
    stage: PreparedStage,
    *,
    sort: str,
    top: int,
    profile_path: Path | None,
) -> StageReport:
    """Execute and profile a single prepared stage."""
    profiler = cProfile.Profile()
    start = time.perf_counter()
    summary = profiler.runcall(stage.workload)
    elapsed_seconds = time.perf_counter() - start

    if profile_path is not None:
        profile_path.parent.mkdir(parents=True, exist_ok=True)
        profiler.dump_stats(str(profile_path))

    return StageReport(
        index=index,
        name=stage.name,
        description=stage.description,
        summary=summary,
        elapsed_seconds=elapsed_seconds,
        profile_path=profile_path,
        stats_text=format_profile_stats(profiler, sort=sort, top=top),
    )


def run_selected_stages(
    jar_path: Path,
    *,
    stage_names: Sequence[str],
    sort: str,
    top: int,
    output_dir: Path | None,
) -> list[StageReport]:
    """Run the requested profiling stages and return their reports."""
    inputs = ProfileInputs(jar_path)
    reports: list[StageReport] = []

    for index, stage_name in enumerate(stage_names, start=1):
        stage = STAGE_BUILDERS[stage_name](inputs)
        profile_path = None if output_dir is None else output_dir / f"{index:02d}-{stage_name}.prof"
        reports.append(run_stage(index, stage, sort=sort, top=top, profile_path=profile_path))

    return reports


def stage_averages(jar_reports: Sequence[JarProfileReport]) -> dict[str, StageAggregate]:
    """Compute aggregate elapsed-time stats for each profiled stage."""
    elapsed_by_stage: dict[str, list[float]] = {}
    for jar_report in jar_reports:
        for stage_report in jar_report.stage_reports:
            elapsed_by_stage.setdefault(stage_report.name, []).append(stage_report.elapsed_seconds)

    aggregates: dict[str, StageAggregate] = {}
    for stage_name, elapsed_values in elapsed_by_stage.items():
        aggregates[stage_name] = StageAggregate(
            count=len(elapsed_values),
            mean_seconds=sum(elapsed_values) / len(elapsed_values),
            min_seconds=min(elapsed_values),
            max_seconds=max(elapsed_values),
        )
    return aggregates


def _output_dirs_for_jars(jar_paths: Sequence[Path], output_dir: Path) -> dict[Path, Path]:
    """Assign each JAR a deterministic output subdirectory."""
    stem_counts = Counter(jar_path.stem for jar_path in jar_paths)
    emitted: Counter[str] = Counter()
    subdirs: dict[Path, Path] = {}

    for jar_path in jar_paths:
        stem = jar_path.stem
        if stem_counts[stem] == 1:
            subdir_name = stem
        else:
            emitted[stem] += 1
            subdir_name = f"{stem}-{emitted[stem]}"
        subdirs[jar_path] = output_dir / subdir_name

    return subdirs


def run_jar_corpus(
    jar_paths: Sequence[Path],
    *,
    stage_names: Sequence[str],
    sort: str,
    top: int,
    output_dir: Path | None,
) -> CorpusReport:
    """Profile the requested stages for every JAR in *jar_paths*."""
    subdirs = {} if output_dir is None else _output_dirs_for_jars(jar_paths, output_dir)
    jar_reports: list[JarProfileReport] = []

    for jar_path in jar_paths:
        stage_reports = run_selected_stages(
            jar_path,
            stage_names=stage_names,
            sort=sort,
            top=top,
            output_dir=subdirs.get(jar_path),
        )
        jar_reports.append(JarProfileReport(jar_path=jar_path, stage_reports=stage_reports))

    return CorpusReport(jars=jar_reports, stage_averages=stage_averages(jar_reports))


def corpus_report_to_json(report: CorpusReport) -> dict[str, object]:
    """Convert a corpus report to a JSON-serializable structure."""
    return {
        "jars": [
            {
                "jar_path": str(jar_report.jar_path),
                "stages": {
                    stage_report.name: {
                        "elapsed_seconds": stage_report.elapsed_seconds,
                        "summary": stage_report.summary,
                        "profile_path": None if stage_report.profile_path is None else str(stage_report.profile_path),
                    }
                    for stage_report in jar_report.stage_reports
                },
            }
            for jar_report in report.jars
        ],
        "stage_averages": {
            stage_name: {
                "count": aggregate.count,
                "mean_seconds": aggregate.mean_seconds,
                "min_seconds": aggregate.min_seconds,
                "max_seconds": aggregate.max_seconds,
            }
            for stage_name, aggregate in report.stage_averages.items()
        },
    }


def write_summary_json(report: CorpusReport, summary_json: Path) -> None:
    """Write a corpus report summary to disk as JSON."""
    summary_json.parent.mkdir(parents=True, exist_ok=True)
    summary_json.write_text(json.dumps(corpus_report_to_json(report), indent=2), encoding="utf-8")


def print_stage_report(report: StageReport) -> None:
    """Print a human-readable summary for a completed stage."""
    print(f"== Stage {report.index}: {report.name} ==")
    print(report.description)
    print(f"elapsed: {report.elapsed_seconds:.6f}s")
    print(f"summary: {report.summary}")
    if report.profile_path is not None:
        print(f"profile: {report.profile_path}")
    print(report.stats_text)
    print()


def print_corpus_report(
    report: CorpusReport,
    *,
    stage_names: Sequence[str],
    output_dir: Path | None,
    summary_json: Path | None,
) -> None:
    """Print a concise human-readable summary for a corpus run."""
    print(f"jars: {len(report.jars)}")
    print(f"stages: {', '.join(stage_names)}")
    print(f"profile_output_dir: {output_dir if output_dir is not None else 'disabled'}")
    if summary_json is not None:
        print(f"summary_json: {summary_json}")
    print()

    for jar_report in report.jars:
        print(f"== Jar: {jar_report.jar_path} ==")
        for stage_report in jar_report.stage_reports:
            print(f"{stage_report.name}: {stage_report.elapsed_seconds:.6f}s")
            print(f"summary: {stage_report.summary}")
            if stage_report.profile_path is not None:
                print(f"profile: {stage_report.profile_path}")
        print()

    print("== Stage averages ==")
    for stage_name in stage_names:
        aggregate = report.stage_averages[stage_name]
        print(
            f"{stage_name}: "
            f"mean={aggregate.mean_seconds:.6f}s "
            f"min={aggregate.min_seconds:.6f}s "
            f"max={aggregate.max_seconds:.6f}s "
            f"count={aggregate.count}"
        )


def main(argv: Sequence[str] | None = None) -> int:
    """Run the profiling harness for one jar or a directory of jars."""
    args = parse_args(argv)
    single_jar_mode = is_single_jar_input(args.inputs)
    stage_names = default_stage_names(args.inputs, args.stages)
    try:
        jar_paths = expand_jar_inputs(args.inputs, recursive=args.recursive)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc

    output_dir: Path | None
    if args.no_save:
        output_dir = None
    elif args.output_dir is not None:
        output_dir = args.output_dir.expanduser()
    elif single_jar_mode:
        output_dir = default_output_dir(jar_paths[0])
    else:
        output_dir = DEFAULT_CORPUS_OUTPUT_DIR

    summary_json = None if args.summary_json is None else args.summary_json.expanduser()

    if single_jar_mode:
        jar_path = jar_paths[0]
        reports = run_selected_stages(
            jar_path,
            stage_names=stage_names,
            sort=args.sort,
            top=args.top,
            output_dir=output_dir,
        )
        print(f"jar: {jar_path}")
        print(f"stages: {', '.join(stage_names)}")
        print(f"profile_output_dir: {output_dir if output_dir is not None else 'disabled'}")
        if summary_json is not None:
            single_report = CorpusReport(
                jars=[JarProfileReport(jar_path=jar_path, stage_reports=reports)],
                stage_averages=stage_averages([JarProfileReport(jar_path=jar_path, stage_reports=reports)]),
            )
            write_summary_json(single_report, summary_json)
            print(f"summary_json: {summary_json}")
        print()
        for report in reports:
            print_stage_report(report)
        return 0

    report = run_jar_corpus(
        jar_paths,
        stage_names=stage_names,
        sort=args.sort,
        top=args.top,
        output_dir=output_dir,
    )
    if summary_json is not None:
        write_summary_json(report, summary_json)

    print_corpus_report(
        report,
        stage_names=stage_names,
        output_dir=output_dir,
        summary_json=summary_json,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
