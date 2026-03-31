import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashSet;
import java.util.List;

import org.objectweb.asm.ClassReader;
import org.objectweb.asm.tree.AbstractInsnNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.InsnList;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.TryCatchBlockNode;
import org.objectweb.asm.tree.analysis.Analyzer;
import org.objectweb.asm.tree.analysis.BasicInterpreter;
import org.objectweb.asm.tree.analysis.BasicValue;
import org.objectweb.asm.util.Printer;

/**
 * JVM-side CFG oracle for pytecode differential testing.
 *
 * Records ASM Analyzer normal and exceptional control-flow edges and emits them
 * as JSON for Python-side normalization and comparison.
 */
public final class RecordingAnalyzer {
    private static final class InstructionRecord {
        final int index;
        final int opcode;
        final String mnemonic;

        InstructionRecord(int index, int opcode, String mnemonic) {
            this.index = index;
            this.opcode = opcode;
            this.mnemonic = mnemonic;
        }
    }

    private static final class NormalEdgeRecord {
        final int from;
        final int to;

        NormalEdgeRecord(int from, int to) {
            this.from = from;
            this.to = to;
        }
    }

    private static final class ExceptionEdgeRecord {
        final int from;
        final int handler;
        final String catchType;

        ExceptionEdgeRecord(int from, int handler, String catchType) {
            this.from = from;
            this.handler = handler;
            this.catchType = catchType;
        }
    }

    private static final class TryCatchBlockRecord {
        final int startIndex;
        final int endIndex;
        final int handlerIndex;
        final String catchType;

        TryCatchBlockRecord(int startIndex, int endIndex, int handlerIndex, String catchType) {
            this.startIndex = startIndex;
            this.endIndex = endIndex;
            this.handlerIndex = handlerIndex;
            this.catchType = catchType;
        }
    }

    private static final class MethodRecording {
        final String className;
        final String methodName;
        final String methodDescriptor;
        final List<InstructionRecord> instructions = new ArrayList<InstructionRecord>();
        final List<NormalEdgeRecord> normalEdges = new ArrayList<NormalEdgeRecord>();
        final List<ExceptionEdgeRecord> exceptionEdges = new ArrayList<ExceptionEdgeRecord>();
        final List<TryCatchBlockRecord> tryCatchBlocks = new ArrayList<TryCatchBlockRecord>();

        private final LinkedHashSet<String> normalEdgeKeys = new LinkedHashSet<String>();
        private final LinkedHashSet<String> exceptionEdgeKeys = new LinkedHashSet<String>();

        MethodRecording(String className, String methodName, String methodDescriptor) {
            this.className = className;
            this.methodName = methodName;
            this.methodDescriptor = methodDescriptor;
        }

        void addNormalEdge(int from, int to) {
            String key = from + ":" + to;
            if (normalEdgeKeys.add(key)) {
                normalEdges.add(new NormalEdgeRecord(from, to));
            }
        }

        void addExceptionEdge(int from, int handler, String catchType) {
            String key = from + ":" + handler + ":" + String.valueOf(catchType);
            if (exceptionEdgeKeys.add(key)) {
                exceptionEdges.add(new ExceptionEdgeRecord(from, handler, catchType));
            }
        }
    }

    private static final class AnalyzerRecorder extends Analyzer<BasicValue> {
        private final InsnList instructions;
        private final int[] asmToRealIndex;
        private final int realInstructionCount;
        private final MethodRecording recording;

        AnalyzerRecorder(InsnList instructions, int[] asmToRealIndex, int realInstructionCount, MethodRecording recording) {
            super(new BasicInterpreter());
            this.instructions = instructions;
            this.asmToRealIndex = asmToRealIndex;
            this.realInstructionCount = realInstructionCount;
            this.recording = recording;
        }

        @Override
        protected void newControlFlowEdge(final int insn, final int successor) {
            int from = asmToRealIndex[insn];
            if (from < 0) {
                return;
            }
            int to = resolveRealInstructionIndex(successor, false);
            recording.addNormalEdge(from, to);
        }

        @Override
        protected boolean newControlFlowExceptionEdge(final int insn, final TryCatchBlockNode tcb) {
            int from = asmToRealIndex[insn];
            if (from >= 0) {
                int handlerAsmIndex = instructions.indexOf(tcb.handler);
                int handler = resolveRealInstructionIndex(handlerAsmIndex, false);
                recording.addExceptionEdge(from, handler, tcb.type);
            }
            return true;
        }

        private int resolveRealInstructionIndex(int asmIndex, boolean allowEnd) {
            for (int i = asmIndex; i < asmToRealIndex.length; i++) {
                if (asmToRealIndex[i] >= 0) {
                    return asmToRealIndex[i];
                }
            }
            if (allowEnd) {
                return realInstructionCount;
            }
            throw new IllegalStateException("Expected a real instruction at or after ASM index " + asmIndex);
        }
    }

    private RecordingAnalyzer() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 1 || args.length > 2) {
            throw new IllegalArgumentException("Usage: RecordingAnalyzer <class-file> [method-name]");
        }

        Path classFile = Paths.get(args[0]);
        String methodFilter = args.length == 2 ? args[1] : null;

        byte[] classBytes = Files.readAllBytes(classFile);
        ClassReader classReader = new ClassReader(classBytes);
        ClassNode classNode = new ClassNode();
        classReader.accept(classNode, ClassReader.SKIP_FRAMES);

        List<MethodRecording> methods = new ArrayList<MethodRecording>();
        for (MethodNode method : classNode.methods) {
            if (methodFilter != null && !method.name.equals(methodFilter)) {
                continue;
            }
            methods.add(analyzeMethod(classNode.name, method));
        }

        StringBuilder json = new StringBuilder();
        appendClassJson(json, classNode.name, methods);
        System.out.println(json.toString());
    }

    private static MethodRecording analyzeMethod(String className, MethodNode method) throws Exception {
        int[] asmToRealIndex = buildRealInstructionIndexMap(method.instructions);
        int realInstructionCount = countRealInstructions(asmToRealIndex);

        MethodRecording recording = new MethodRecording(className, method.name, method.desc);
        appendInstructionRecords(method.instructions, asmToRealIndex, recording);

        AnalyzerRecorder analyzer = new AnalyzerRecorder(method.instructions, asmToRealIndex, realInstructionCount, recording);
        analyzer.analyze(className, method);

        for (TryCatchBlockNode tryCatchBlock : method.tryCatchBlocks) {
            int startIndex = resolveRealInstructionIndex(method.instructions, asmToRealIndex, realInstructionCount, method.instructions.indexOf(tryCatchBlock.start), false);
            int endIndex = resolveRealInstructionIndex(method.instructions, asmToRealIndex, realInstructionCount, method.instructions.indexOf(tryCatchBlock.end), true);
            int handlerIndex = resolveRealInstructionIndex(method.instructions, asmToRealIndex, realInstructionCount, method.instructions.indexOf(tryCatchBlock.handler), false);
            recording.tryCatchBlocks.add(new TryCatchBlockRecord(startIndex, endIndex, handlerIndex, tryCatchBlock.type));
        }

        return recording;
    }

    private static void appendInstructionRecords(InsnList instructions, int[] asmToRealIndex, MethodRecording recording) {
        for (int i = 0; i < instructions.size(); i++) {
            AbstractInsnNode instruction = instructions.get(i);
            if (!isRealInstruction(instruction)) {
                continue;
            }
            int opcode = instruction.getOpcode();
            recording.instructions.add(new InstructionRecord(asmToRealIndex[i], opcode, opcodeMnemonic(opcode)));
        }
    }

    private static int[] buildRealInstructionIndexMap(InsnList instructions) {
        int[] asmToRealIndex = new int[instructions.size()];
        Arrays.fill(asmToRealIndex, -1);

        int realIndex = 0;
        for (int i = 0; i < instructions.size(); i++) {
            if (isRealInstruction(instructions.get(i))) {
                asmToRealIndex[i] = realIndex;
                realIndex++;
            }
        }
        return asmToRealIndex;
    }

    private static int countRealInstructions(int[] asmToRealIndex) {
        int count = 0;
        for (int index : asmToRealIndex) {
            if (index >= 0) {
                count++;
            }
        }
        return count;
    }

    private static int resolveRealInstructionIndex(
            InsnList instructions,
            int[] asmToRealIndex,
            int realInstructionCount,
            int asmIndex,
            boolean allowEnd) {
        for (int i = asmIndex; i < instructions.size(); i++) {
            if (asmToRealIndex[i] >= 0) {
                return asmToRealIndex[i];
            }
        }
        if (allowEnd) {
            return realInstructionCount;
        }
        throw new IllegalStateException("Expected a real instruction at or after ASM index " + asmIndex);
    }

    private static boolean isRealInstruction(AbstractInsnNode instruction) {
        int type = instruction.getType();
        return type != AbstractInsnNode.LABEL
                && type != AbstractInsnNode.LINE
                && type != AbstractInsnNode.FRAME;
    }

    private static String opcodeMnemonic(int opcode) {
        if (opcode < 0) {
            return "UNKNOWN";
        }
        if (opcode < Printer.OPCODES.length && Printer.OPCODES[opcode] != null) {
            return Printer.OPCODES[opcode];
        }
        return Integer.toString(opcode);
    }

    private static void appendClassJson(StringBuilder json, String className, List<MethodRecording> methods) {
        json.append('{');
        json.append("\"className\":");
        appendJsonString(json, className);
        json.append(",\"methods\":[");
        for (int i = 0; i < methods.size(); i++) {
            if (i > 0) {
                json.append(',');
            }
            appendMethodJson(json, methods.get(i));
        }
        json.append("]}");
    }

    private static void appendMethodJson(StringBuilder json, MethodRecording method) {
        json.append('{');
        json.append("\"className\":");
        appendJsonString(json, method.className);
        json.append(",\"methodName\":");
        appendJsonString(json, method.methodName);
        json.append(",\"methodDescriptor\":");
        appendJsonString(json, method.methodDescriptor);
        json.append(",\"instructions\":[");
        for (int i = 0; i < method.instructions.size(); i++) {
            if (i > 0) {
                json.append(',');
            }
            InstructionRecord instruction = method.instructions.get(i);
            json.append('{');
            json.append("\"index\":").append(instruction.index);
            json.append(",\"opcode\":").append(instruction.opcode);
            json.append(",\"mnemonic\":");
            appendJsonString(json, instruction.mnemonic);
            json.append('}');
        }
        json.append("],\"normalEdges\":[");
        for (int i = 0; i < method.normalEdges.size(); i++) {
            if (i > 0) {
                json.append(',');
            }
            NormalEdgeRecord edge = method.normalEdges.get(i);
            json.append('{');
            json.append("\"from\":").append(edge.from);
            json.append(",\"to\":").append(edge.to);
            json.append('}');
        }
        json.append("],\"exceptionEdges\":[");
        for (int i = 0; i < method.exceptionEdges.size(); i++) {
            if (i > 0) {
                json.append(',');
            }
            ExceptionEdgeRecord edge = method.exceptionEdges.get(i);
            json.append('{');
            json.append("\"from\":").append(edge.from);
            json.append(",\"handler\":").append(edge.handler);
            json.append(",\"catchType\":");
            appendNullableJsonString(json, edge.catchType);
            json.append('}');
        }
        json.append("],\"tryCatchBlocks\":[");
        for (int i = 0; i < method.tryCatchBlocks.size(); i++) {
            if (i > 0) {
                json.append(',');
            }
            TryCatchBlockRecord block = method.tryCatchBlocks.get(i);
            json.append('{');
            json.append("\"startIndex\":").append(block.startIndex);
            json.append(",\"endIndex\":").append(block.endIndex);
            json.append(",\"handlerIndex\":").append(block.handlerIndex);
            json.append(",\"catchType\":");
            appendNullableJsonString(json, block.catchType);
            json.append('}');
        }
        json.append("]}");
    }

    private static void appendNullableJsonString(StringBuilder json, String value) {
        if (value == null) {
            json.append("null");
            return;
        }
        appendJsonString(json, value);
    }

    private static void appendJsonString(StringBuilder json, String value) {
        json.append('"');
        for (int i = 0; i < value.length(); i++) {
            char ch = value.charAt(i);
            switch (ch) {
                case '"':
                    json.append("\\\"");
                    break;
                case '\\':
                    json.append("\\\\");
                    break;
                case '\b':
                    json.append("\\b");
                    break;
                case '\f':
                    json.append("\\f");
                    break;
                case '\n':
                    json.append("\\n");
                    break;
                case '\r':
                    json.append("\\r");
                    break;
                case '\t':
                    json.append("\\t");
                    break;
                default:
                    if (ch < 0x20) {
                        json.append(String.format("\\u%04x", (int) ch));
                    } else {
                        json.append(ch);
                    }
                    break;
            }
        }
        json.append('"');
    }
}
