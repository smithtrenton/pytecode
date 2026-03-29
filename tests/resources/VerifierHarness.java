import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * JVM verification harness for pytecode validation testing.
 *
 * Reads a .class file, extracts the class name from its constant pool,
 * defines it via a custom ClassLoader, and reports the verification result
 * as structured JSON on stdout.
 *
 * Usage:
 *   java -Xverify:all VerifierHarness &lt;path.class&gt;
 *   java -Xverify:all VerifierHarness &lt;path.class&gt; execute &lt;className&gt; [args...]
 *   java -Xverify:all VerifierHarness batch &lt;path.class&gt;...
 *
 * Output (one JSON line):
 *   {"status":"VERIFY_OK"}
 *   {"status":"VERIFY_OK","stdout":"..."}          (execute mode)
 *   {"status":"VERIFY_FAIL","message":"..."}
 *   {"status":"FORMAT_FAIL","message":"..."}
 *   [{"path":"...","status":"VERIFY_OK"}, ...]     (batch mode)
 */
public class VerifierHarness {

    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.out.println("{\"status\":\"FORMAT_FAIL\",\"message\":\"usage: VerifierHarness <path.class> [execute <className> [args...]] | VerifierHarness batch <path.class>...\"}");
            System.exit(1);
        }

        if ("batch".equals(args[0])) {
            if (args.length < 2) {
                System.out.println("{\"status\":\"FORMAT_FAIL\",\"message\":\"usage: VerifierHarness batch <path.class>...\"}");
                System.exit(1);
                return;
            }
            System.out.println(verifyBatch(args));
            return;
        }

        Path classFilePath = Path.of(args[0]);
        byte[] classBytes = Files.readAllBytes(classFilePath);

        boolean executeMode = args.length >= 3 && "execute".equals(args[1]);
        String className;
        String[] execArgs = new String[0];

        if (executeMode) {
            className = args[2];
            execArgs = new String[args.length - 3];
            System.arraycopy(args, 3, execArgs, 0, execArgs.length);
        } else {
            className = extractClassName(classBytes);
            if (className == null) {
                System.out.println("{\"status\":\"FORMAT_FAIL\",\"message\":\"cannot extract class name from constant pool\"}");
                System.exit(1);
                return;
            }
        }

        System.out.println(verifyClass(classFilePath, classBytes, className, executeMode, execArgs, false));
    }

    private static String verifyBatch(String[] args) {
        StringBuilder out = new StringBuilder("[");
        for (int i = 1; i < args.length; i++) {
            if (i > 1) {
                out.append(",");
            }
            Path classFilePath = Path.of(args[i]);
            try {
                byte[] classBytes = Files.readAllBytes(classFilePath);
                String className = extractClassName(classBytes);
                if (className == null) {
                    out.append(resultJson(classFilePath, "FORMAT_FAIL", "cannot extract class name from constant pool", null, null, true));
                } else {
                    out.append(verifyClass(classFilePath, classBytes, className, false, new String[0], true));
                }
            } catch (Exception e) {
                out.append(resultJson(classFilePath, "VERIFY_FAIL", e.toString(), null, null, true));
            }
        }
        out.append("]");
        return out.toString();
    }

    private static String verifyClass(
        Path classFilePath,
        byte[] classBytes,
        String className,
        boolean executeMode,
        String[] execArgs,
        boolean includePath
    ) {
        String dotClassName = className.replace('/', '.');

        try {
            ClassLoader loader = new ClassLoader(VerifierHarness.class.getClassLoader()) {
                @Override
                protected Class<?> findClass(String name) throws ClassNotFoundException {
                    if (name.equals(dotClassName)) {
                        return defineClass(name, classBytes, 0, classBytes.length);
                    }
                    throw new ClassNotFoundException(name);
                }
            };

            Class<?> clazz = loader.loadClass(dotClassName);

            if (executeMode) {
                ByteArrayOutputStream baos = new ByteArrayOutputStream();
                PrintStream capture = new PrintStream(baos);
                PrintStream originalOut = System.out;
                System.setOut(capture);
                try {
                    java.lang.reflect.Method mainMethod = clazz.getMethod("main", String[].class);
                    mainMethod.invoke(null, (Object) execArgs);
                } finally {
                    System.setOut(originalOut);
                    capture.flush();
                }
                String stdout = baos.toString("UTF-8");
                return resultJson(classFilePath, "VERIFY_OK", null, stdout, null, includePath);
            } else {
                return resultJson(classFilePath, "VERIFY_OK", null, null, null, includePath);
            }
        } catch (VerifyError e) {
            return resultJson(classFilePath, "VERIFY_FAIL", e.getMessage(), null, null, includePath);
        } catch (ClassFormatError e) {
            return resultJson(classFilePath, "FORMAT_FAIL", e.getMessage(), null, null, includePath);
        } catch (LinkageError e) {
            return resultJson(classFilePath, "VERIFY_FAIL", e.toString(), null, null, includePath);
        } catch (Exception e) {
            if (executeMode) {
                // Execution failed but class loaded (verification passed)
                return resultJson(classFilePath, "VERIFY_OK", null, null, e.toString(), includePath);
            } else {
                return resultJson(classFilePath, "VERIFY_FAIL", e.toString(), null, null, includePath);
            }
        }
    }

    private static String resultJson(
        Path classFilePath,
        String status,
        String message,
        String stdout,
        String execError,
        boolean includePath
    ) {
        StringBuilder sb = new StringBuilder("{");
        if (includePath) {
            sb.append("\"path\":").append(jsonEscape(classFilePath.toString())).append(",");
        }
        sb.append("\"status\":").append(jsonEscape(status));
        if (message != null) {
            sb.append(",\"message\":").append(jsonEscape(message));
        }
        if (stdout != null) {
            sb.append(",\"stdout\":").append(jsonEscape(stdout));
        }
        if (execError != null) {
            sb.append(",\"exec_error\":").append(jsonEscape(execError));
        }
        sb.append("}");
        return sb.toString();
    }

    /**
     * Extract the class name from the classfile constant pool.
     * Reads this_class (u2 at offset 2 after access_flags) which points to a
     * CONSTANT_Class entry, which in turn points to a CONSTANT_Utf8 entry.
     */
    private static String extractClassName(byte[] bytes) {
        if (bytes.length < 10) return null;

        // Verify magic number
        if ((bytes[0] & 0xFF) != 0xCA || (bytes[1] & 0xFF) != 0xFE ||
            (bytes[2] & 0xFF) != 0xBA || (bytes[3] & 0xFF) != 0xBE) {
            return null;
        }

        int cpCount = readU2(bytes, 8);
        int[] cpOffsets = new int[cpCount];
        int[] cpTags = new int[cpCount];

        int offset = 10;
        for (int i = 1; i < cpCount; i++) {
            if (offset >= bytes.length) return null;
            int tag = bytes[offset] & 0xFF;
            cpTags[i] = tag;
            cpOffsets[i] = offset;
            offset++;

            switch (tag) {
                case 1: // Utf8
                    int len = readU2(bytes, offset);
                    offset += 2 + len;
                    break;
                case 3: case 4: // Integer, Float
                    offset += 4;
                    break;
                case 5: case 6: // Long, Double
                    offset += 8;
                    i++; // double-slot
                    break;
                case 7: case 8: case 16: case 19: case 20: // Class, String, MethodType, Module, Package
                    offset += 2;
                    break;
                case 9: case 10: case 11: case 12: case 17: case 18:
                    // Fieldref, Methodref, InterfaceMethodref, NameAndType, Dynamic, InvokeDynamic
                    offset += 4;
                    break;
                case 15: // MethodHandle
                    offset += 3;
                    break;
                default:
                    return null;
            }
        }

        // access_flags at offset, this_class at offset+2
        int thisClassIndex = readU2(bytes, offset + 2);
        if (thisClassIndex <= 0 || thisClassIndex >= cpCount
                || cpOffsets[thisClassIndex] == 0 || cpTags[thisClassIndex] != 7) {
            return null;
        }

        int nameIndex = readU2(bytes, cpOffsets[thisClassIndex] + 1);
        if (nameIndex <= 0 || nameIndex >= cpCount
                || cpOffsets[nameIndex] == 0 || cpTags[nameIndex] != 1) {
            return null;
        }

        int utf8Offset = cpOffsets[nameIndex] + 1;
        int utf8Len = readU2(bytes, utf8Offset);
        return new String(bytes, utf8Offset + 2, utf8Len);
    }

    private static int readU2(byte[] bytes, int offset) {
        return ((bytes[offset] & 0xFF) << 8) | (bytes[offset + 1] & 0xFF);
    }

    private static String jsonEscape(String s) {
        if (s == null) return "null";
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"': sb.append("\\\""); break;
                case '\\': sb.append("\\\\"); break;
                case '\n': sb.append("\\n"); break;
                case '\r': sb.append("\\r"); break;
                case '\t': sb.append("\\t"); break;
                default:
                    if (c < 0x20) {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
            }
        }
        sb.append("\"");
        return sb.toString();
    }
}
