import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;

/**
 * Exercises every non-control-flow instruction family that the pytecode editing
 * model wraps symbolically:
 *   - GETFIELD / PUTFIELD / GETSTATIC / PUTSTATIC  (FieldInsn)
 *   - INVOKEVIRTUAL / INVOKESPECIAL / INVOKESTATIC (MethodInsn)
 *   - INVOKEINTERFACE                              (InterfaceMethodInsn)
 *   - NEW / CHECKCAST / INSTANCEOF / ANEWARRAY    (TypeInsn)
 *   - LDC / LDC_W / LDC2_W constants             (LdcInsn)
 *   - MULTIANEWARRAY                              (MultiANewArrayInsn)
 *   - local variable access beyond slot 3         (VarInsn)
 *   - IINC in both narrow and wide-ish ranges     (IIncInsn)
 *   - INVOKEDYNAMIC via lambda                    (InvokeDynamicInsn)
 */
public class InstructionShowcase {

    // Instance fields exercised by GETFIELD / PUTFIELD
    public int intField;
    private String stringField;

    // Static field exercised by GETSTATIC / PUTSTATIC
    public static long counter = 0L;

    // -----------------------------------------------------------------------
    // Field access (GETFIELD, PUTFIELD, GETSTATIC, PUTSTATIC)
    // -----------------------------------------------------------------------

    public int readField() {
        return this.intField;           // GETFIELD
    }

    public void writeField(int value) {
        this.intField = value;          // PUTFIELD
    }

    public static long readStatic() {
        return counter;                 // GETSTATIC
    }

    public static void writeStatic(long value) {
        counter = value;                // PUTSTATIC
    }

    // -----------------------------------------------------------------------
    // Method invocation
    // -----------------------------------------------------------------------

    private String buildString(String prefix) {
        // INVOKESPECIAL (StringBuilder.<init>), INVOKEVIRTUAL (append, toString)
        StringBuilder sb = new StringBuilder();
        sb.append(prefix);
        sb.append(intField);
        return sb.toString();
    }

    public static String staticHelper(String s) {
        // INVOKESTATIC (String.valueOf)
        return String.valueOf(s);
    }

    public int compareViaInterface(List<String> list, String item) {
        // INVOKEINTERFACE (List.size, List.contains)
        int size = list.size();
        boolean found = list.contains(item);
        return found ? size : -1;
    }

    // -----------------------------------------------------------------------
    // Type instructions (NEW, CHECKCAST, INSTANCEOF, ANEWARRAY)
    // -----------------------------------------------------------------------

    public Object typeOps(Object obj) {
        // NEW
        ArrayList<String> fresh = new ArrayList<>();
        // INSTANCEOF
        if (obj instanceof String) {
            // CHECKCAST
            String s = (String) obj;
            fresh.add(s);
        }
        // ANEWARRAY
        String[] arr = new String[3];
        arr[0] = "hello";
        fresh.add(arr[0]);
        return fresh;
    }

    // -----------------------------------------------------------------------
    // LDC / LDC_W / LDC2_W constants
    // -----------------------------------------------------------------------

    public void loadConstants() {
        // LDC int (100_000 exceeds SIPUSH range so javac emits LDC/LDC_W for it)
        int i = 100_000;
        // LDC float (raw bits)
        float f = 3.14f;
        // LDC2_W long
        long l = 1234567890123L;
        // LDC2_W double
        double d = 2.718281828;
        // LDC string
        String s = "hello pytecode";
        // LDC class literal
        Class<?> cls = String.class;
        // Use them to avoid dead-code elimination
        counter = i + (long) f + l + (long) d + s.length() + cls.getName().length();
    }

    // -----------------------------------------------------------------------
    // MULTIANEWARRAY
    // -----------------------------------------------------------------------

    public int[][] multiArray(int rows, int cols) {
        // MULTIANEWARRAY [[I 2
        return new int[rows][cols];
    }

    // -----------------------------------------------------------------------
    // Local variable access beyond slot 3 (VarInsn with slot > 3)
    // -----------------------------------------------------------------------

    public long manyLocals(int a, int b, int c, int d, int e, int f) {
        // Parameters occupy slots 1-6 (slot 0 = this).
        // Local vars below push some slots further out.
        int x = a + b;       // slot 7
        int y = c + d;       // slot 8
        long z = e + f;      // slots 9-10 (long = 2 slots)
        long w = z * 2;      // slot 11-12
        // ILOAD slot 7, slot 8 → explicit LocalIndex forms
        return x + y + z + w;
    }

    // -----------------------------------------------------------------------
    // IINC
    // -----------------------------------------------------------------------

    public int iincDemo(int start) {
        // IINC slot 1 (parameter) by +1 and -1
        start++;
        start--;
        start += 100;
        return start;
    }

    // -----------------------------------------------------------------------
    // INVOKEDYNAMIC via lambda (generates invokedynamic + BootstrapMethods)
    // -----------------------------------------------------------------------

    public Supplier<String> makeLambda(String msg) {
        // INVOKEDYNAMIC with LambdaMetafactory bootstrap
        return () -> msg;
    }
}
