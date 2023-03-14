from collections import namedtuple
from pprint import pprint

import requests
from bs4 import BeautifulSoup as bs

Inst = namedtuple("Inst", ["name", "hex", "operands"])


def extract_inst(row):
    cells = row.find_all("td")
    [n.replaceWith("") for n in cells[0].find_all("sup")]
    return Inst(*[cells[i].get_text(strip=True) for i in (0, 1, 3)])


url = "https://en.wikipedia.org/wiki/List_of_Java_bytecode_instructions"
page = requests.get(url)
soup = bs(page.content, "html.parser")

table = soup.find("table", class_="wikitable")
body = table.find("tbody")
rows = body.find_all("tr")
header, no_name = rows[0], extract_inst(rows[-1])
inst_rows = rows[1:-1]

instructions = [extract_inst(i) for i in inst_rows]

for inst in instructions:
    pprint(inst)
pprint(no_name)

print("")
for inst in instructions:
    operands_str = None
    parsed_operands = [o for o in inst.operands.split(": ")[-1].split(", ")]
    match (inst.name, parsed_operands):
        case x if x[0] == "lookupswitch":
            operands_str = "LookupSwitch"
        case x if x[0] == "tableswitch":
            operands_str = "TableSwitch"
        case x if x[0] == "multianewarray":
            operands_str = "MultiANewArray"
        case x if x[0] == "newarray":
            operands_str = "NewArray"
        case x if x[0] == "invokedynamic":
            operands_str = "InvokeDynamic"
        case x if x[0] == "invokeinterface":
            operands_str = "InvokeInterface"
        case x if x[0] == "iinc":
            operands_str = "IInc"
        case x if x[0] == "bipush":
            operands_str = "ByteValue"
        case x if x[0] == "sipush":
            operands_str = "ShortValue"
        case x if x[1] == ["index"]:
            operands_str = "LocalIndex"
        case x if x[1] == ["branchbyte1", "branchbyte2"]:
            operands_str = "Branch"
        case x if x[1] == ["indexbyte1", "indexbyte2"]:
            operands_str = "ConstPoolIndex"
        case x if x[1] == ["branchbyte1", "branchbyte2", "branchbyte3", "branchbyte4"]:
            operands_str = "WideBranch"
        case default:
            operands_str = "InsnInfo"

    print(f"\t{inst.name.upper()} = 0x{inst.hex}, {operands_str}")

import IPython

IPython.embed()
