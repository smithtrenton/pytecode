/**
 * Comprehensive control-flow fixture for pytecode CFG analysis tests.
 *
 * Exercises: straight-line code, if/else, loops, dense/sparse switches,
 * try-catch, try-finally, nested exceptions, dead code, multi-return,
 * category-2 operations, stack manipulation, object creation, and more.
 */
public class CfgFixture {

    // --- Straight-line code (single basic block) ---
    public static int straightLine(int a, int b) {
        int c = a + b;
        int d = c * 2;
        return d;
    }

    // --- Empty method ---
    public static void emptyMethod() {
    }

    // --- If/else branching ---
    public int ifElse(boolean flag, int x) {
        if (flag) {
            return x + 1;
        } else {
            return x - 1;
        }
    }

    // --- If with no else (fall-through) ---
    public int ifNoElse(int x) {
        if (x > 0) {
            x = x * 2;
        }
        return x;
    }

    // --- For loop ---
    public int forLoop(int limit) {
        int sum = 0;
        for (int i = 0; i < limit; i++) {
            sum += i;
        }
        return sum;
    }

    // --- While loop ---
    public int whileLoop(int n) {
        int result = 1;
        while (n > 0) {
            result *= n;
            n--;
        }
        return result;
    }

    // --- Nested loops ---
    public int nestedLoops(int rows, int cols) {
        int total = 0;
        for (int i = 0; i < rows; i++) {
            for (int j = 0; j < cols; j++) {
                total += i * cols + j;
            }
        }
        return total;
    }

    // --- Dense switch (tableswitch) ---
    public int denseSwitch(int value) {
        switch (value) {
            case 0: return 10;
            case 1: return 20;
            case 2: return 30;
            case 3: return 40;
            default: return -1;
        }
    }

    // --- Sparse switch (lookupswitch) ---
    public int sparseSwitch(int value) {
        switch (value) {
            case -100: return 1;
            case 0: return 2;
            case 500: return 3;
            case 10000: return 4;
            default: return -1;
        }
    }

    // --- Try-catch (single handler) ---
    public int tryCatchSingle(int a, int b) {
        try {
            return a / b;
        } catch (ArithmeticException e) {
            return -1;
        }
    }

    // --- Try-catch (multiple handlers) ---
    public String tryCatchMultiple(Object obj) {
        try {
            return obj.toString();
        } catch (NullPointerException e) {
            return "null";
        } catch (RuntimeException e) {
            return "runtime";
        }
    }

    // --- Try-catch-finally ---
    public int tryCatchFinally(int a, int b) {
        int result;
        try {
            result = a / b;
        } catch (ArithmeticException e) {
            result = -1;
        } finally {
            System.out.println("done");
        }
        return result;
    }

    // --- Nested try-catch ---
    public int nestedTryCatch(int a, int b, int c) {
        try {
            try {
                return a / b;
            } catch (ArithmeticException e1) {
                return a / c;
            }
        } catch (ArithmeticException e2) {
            return 0;
        }
    }

    // --- Category-2 operations (long/double) ---
    public long longArithmetic(long a, long b) {
        long sum = a + b;
        long product = a * b;
        return sum - product;
    }

    public double doubleArithmetic(double a, double b) {
        double sum = a + b;
        double diff = a - b;
        return sum * diff;
    }

    // --- Type conversions ---
    public double mixedConversions(int i, long l, float f) {
        double d1 = (double) i;
        double d2 = (double) l;
        double d3 = (double) f;
        return d1 + d2 + d3;
    }

    // --- Object creation with NEW + <init> ---
    public Object createObject() {
        return new Object();
    }

    public String createString() {
        StringBuilder sb = new StringBuilder();
        sb.append("hello");
        sb.append(" world");
        return sb.toString();
    }

    // --- Array operations ---
    public int[] createIntArray(int size) {
        return new int[size];
    }

    public String[] createStringArray(int size) {
        return new String[size];
    }

    public int arrayAccess(int[] arr, int idx) {
        return arr[idx];
    }

    public void arrayStore(int[] arr, int idx, int value) {
        arr[idx] = value;
    }

    // --- INSTANCEOF and CHECKCAST ---
    public boolean isString(Object obj) {
        return obj instanceof String;
    }

    public String castToString(Object obj) {
        return (String) obj;
    }

    // --- Multiple return paths ---
    public int multipleReturns(int x) {
        if (x < 0) return -1;
        if (x == 0) return 0;
        if (x > 100) return 100;
        return x;
    }

    // --- Method calls (virtual, static, interface) ---
    public int methodCalls(String s) {
        int len = s.length();
        int hash = s.hashCode();
        int abs = Math.abs(len - hash);
        return abs;
    }

    // --- Static initializer ---
    static int STATIC_FIELD;
    static {
        STATIC_FIELD = 42;
    }

    // --- Monitor (synchronized) ---
    public synchronized int synchronizedMethod(int x) {
        return x + 1;
    }

    // --- Null check pattern ---
    public String nullCheck(Object obj) {
        if (obj == null) {
            return "null";
        }
        return obj.toString();
    }

    // --- Boolean logic ---
    public boolean complexCondition(int a, int b, boolean flag) {
        return (a > 0 && b > 0) || flag;
    }

    // --- Multi-dimensional array ---
    public int[][] create2DArray(int rows, int cols) {
        return new int[rows][cols];
    }

    // --- Void method with side effects ---
    public void voidMethod(int x) {
        if (x > 0) {
            System.out.println("positive");
        } else {
            System.out.println("non-positive");
        }
    }

    // --- Long comparison ---
    public int compareLongs(long a, long b) {
        if (a < b) return -1;
        if (a > b) return 1;
        return 0;
    }

    // --- Float/double comparison ---
    public int compareDoubles(double a, double b) {
        if (a < b) return -1;
        if (a > b) return 1;
        return 0;
    }

    // --- ATHROW ---
    public void throwException() {
        throw new RuntimeException("error");
    }
}
