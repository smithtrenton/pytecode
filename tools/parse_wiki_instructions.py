from __future__ import annotations

import argparse
from collections import namedtuple
from collections.abc import Sequence
from html.parser import HTMLParser
from pathlib import Path
from pprint import pformat
from urllib.request import urlopen

Inst = namedtuple("Inst", ["name", "hex", "operands"])
WIKI_URL = "https://en.wikipedia.org/wiki/List_of_Java_bytecode_instructions"


class InstructionTableParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.rows: list[list[str]] = []
        self._capturing_table = False
        self._table_depth = 0
        self._current_row: list[str] | None = None
        self._current_cell: list[str] | None = None
        self._ignore_depth = 0

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attr_map = dict(attrs)

        if tag == "table":
            classes = set((attr_map.get("class") or "").split())
            if self._capturing_table:
                self._table_depth += 1
            elif "wikitable" in classes:
                self._capturing_table = True
                self._table_depth = 1
            return

        if not self._capturing_table:
            return

        if tag == "tr":
            self._current_row = []
        elif tag in {"td", "th"}:
            self._current_cell = []
        elif tag == "sup" and self._current_cell is not None:
            self._ignore_depth += 1
        elif tag == "br" and self._current_cell is not None and self._ignore_depth == 0:
            self._current_cell.append(" ")

    def handle_endtag(self, tag: str) -> None:
        if tag == "table" and self._capturing_table:
            self._table_depth -= 1
            if self._table_depth == 0:
                self._capturing_table = False
            return

        if not self._capturing_table:
            return

        if tag == "sup" and self._ignore_depth > 0:
            self._ignore_depth -= 1
        elif tag in {"td", "th"} and self._current_cell is not None:
            cell_text = " ".join("".join(self._current_cell).split())
            if self._current_row is None:
                raise ValueError("Encountered a table cell outside of a row.")
            self._current_row.append(cell_text)
            self._current_cell = None
        elif tag == "tr" and self._current_row is not None:
            if any(cell for cell in self._current_row):
                self.rows.append(self._current_row)
            self._current_row = None

    def handle_data(self, data: str) -> None:
        if self._current_cell is not None and self._ignore_depth == 0:
            self._current_cell.append(data)


def extract_inst(row: list[str]) -> Inst:
    return Inst(row[0], row[1], row[3])


def parse_instruction_table(html: str) -> tuple[list[Inst], Inst]:
    parser = InstructionTableParser()
    parser.feed(html)
    parser.close()

    if len(parser.rows) < 3:
        raise ValueError("Unable to find the instruction table in the provided HTML.")

    no_name = extract_inst(parser.rows[-1])
    instructions = [extract_inst(row) for row in parser.rows[1:-1]]
    return instructions, no_name


def operand_type_for_instruction(inst: Inst) -> str:
    parsed_operands = inst.operands.split(": ")[-1].split(", ")

    match (inst.name, parsed_operands):
        case ("lookupswitch", _):
            return "LookupSwitch"
        case ("tableswitch", _):
            return "TableSwitch"
        case ("multianewarray", _):
            return "MultiANewArray"
        case ("newarray", _):
            return "NewArray"
        case ("invokedynamic", _):
            return "InvokeDynamic"
        case ("invokeinterface", _):
            return "InvokeInterface"
        case ("iinc", _):
            return "IInc"
        case ("bipush", _):
            return "ByteValue"
        case ("sipush", _):
            return "ShortValue"
        case (_, ["index"]):
            return "LocalIndex"
        case (_, ["branchbyte1", "branchbyte2"]):
            return "Branch"
        case (_, ["indexbyte1", "indexbyte2"]):
            return "ConstPoolIndex"
        case (_, ["branchbyte1", "branchbyte2", "branchbyte3", "branchbyte4"]):
            return "WideBranch"
        case _:
            return "InsnInfo"


def render_instruction_dump(instructions: Sequence[Inst], no_name: Inst) -> str:
    lines = [*(pformat(inst) for inst in instructions), pformat(no_name), ""]
    lines.extend(f"\t{inst.name.upper()} = 0x{inst.hex}, {operand_type_for_instruction(inst)}" for inst in instructions)
    return "\n".join(lines) + "\n"


def fetch_wiki_html(url: str = WIKI_URL) -> str:
    with urlopen(url) as response:
        return response.read().decode("utf-8")


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate instruction enum helpers from the Java bytecode instruction table."
    )
    parser.add_argument(
        "--input-html",
        type=Path,
        help="Read HTML from a local file instead of fetching Wikipedia.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Write generated output to a file instead of stdout.",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    html = args.input_html.read_text(encoding="utf-8") if args.input_html is not None else fetch_wiki_html()
    instructions, no_name = parse_instruction_table(html)
    rendered = render_instruction_dump(instructions, no_name)

    if args.output is not None:
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
