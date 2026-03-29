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
 *
 * Output (one JSON line):
 *   {"status":"VERIFY_OK"}
 *   {"status":"VERIFY_OK","stdout":"..."}          (execute mode)
 *   {"status":"VERIFY_FAIL","message":"..."}
 *   {"status":"FORMAT_FAIL","message":"..."}
 */
public class VerifierHarness {

    public static void main(String[] args) throws Exception {
        if (args.length < 1) {
            System.out.println("{\"status\":\"FORMAT_FAIL\",\"message\":\"usage: VerifierHarness <path.class> [execute <className> [args...]]\"}");
            System.exit(1);
        }

        Path classFilePath = Path.of(args[0]);
        byte[] classBytes = Files.readAllBytes(classFilePath);

        boolean executeMode = args.length >= 3 && "execute".equals(args[1]);
        String className;

        if (executeMode) {
            className = args[2];
        } else {
            className = extractClassName(classBytes);
            if (className == null) {
                System.out.println("{\"status\":\"FORMAT_FAIL\",\"message\":\"cannot extract class name from constant pool\"}");
                System.exit(1);
                return;
            }
        }

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
                String[] execArgs = new String[args.length - 3];
                System.arraycopy(args, 3, execArgs, 0, execArgs.length);

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
                System.out.println("{\"status\":\"VERIFY_OK\",\"stdout\":" + jsonEscape(stdout) + "}");
            } else {
                System.out.println("{\"status\":\"VERIFY_OK\"}");
            }
        } catch (VerifyError e) {
            System.out.println("{\"status\":\"VERIFY_FAIL\",\"message\":" + jsonEscape(e.getMessage()) + "}");
        } catch (ClassFormatError e) {
            System.out.println("{\"status\":\"FORMAT_FAIL\",\"message\":" + jsonEscape(e.getMessage()) + "}");
        } catch (Exception e) {
            if (executeMode) {
                // Execution failed but class loaded (verification passed)
                System.out.println("{\"status\":\"VERIFY_OK\",\"exec_error\":" + jsonEscape(e.toString()) + "}");
            } else {
                System.out.println("{\"status\":\"VERIFY_FAIL\",\"message\":" + jsonEscape(e.toString()) + "}");
            }
        }
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
        if (thisClassIndex <= 0 || thisClassIndex >= cpCount || cpTags[thisClassIndex] != 7) {
            return null;
        }

        int nameIndex = readU2(bytes, cpOffsets[thisClassIndex] + 1);
        if (nameIndex <= 0 || nameIndex >= cpCount || cpTags[nameIndex] != 1) {
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
