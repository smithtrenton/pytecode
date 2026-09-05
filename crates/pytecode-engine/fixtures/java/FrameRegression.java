class FrameBase {
    protected int seed;
    FrameBase(int seed) { this.seed = seed; }
}

public class FrameRegression extends FrameBase {
    private long count;

    public FrameRegression(boolean flag) {
        super(flag ? 1 : 2);
        if (flag) count = seed;
        else count = seed + 1;
    }

    static int negate(int value) { return -value; }
    static long negate(long value) { return -value; }
    static float negate(float value) { return -value; }
    static double negate(double value) { return -value; }
    static long shiftLeft(long value, int shift) { return value << shift; }
    static long shiftRight(long value, int shift) { return value >> shift; }
    static long shiftUnsigned(long value, int shift) { return value >>> shift; }

    static Object arrayJoin(boolean flag) {
        Object[] values = flag ? new String[] {"ok"} : new Integer[] {7};
        return values[0];
    }

    static int safeDivide(int value, int divisor) {
        try { return value / divisor; }
        catch (ArithmeticException error) { return -1; }
    }

    static int select(int value) {
        switch (value) {
            case 0: return 11;
            case 1: return 12;
            case 2: return 13;
            default: return 14;
        }
    }

    static void require(boolean condition) {
        if (!condition) throw new AssertionError("frame regression");
    }

    public static void main(String[] args) {
        if (wideJoin(10L, true) != 11L || wideJoin(10L, false) != 12L)
            throw new AssertionError("long locals and stack at join");
        if (wideJoin(10.0, true) != 11.0 || wideJoin(10.0, false) != 12.0)
            throw new AssertionError("double locals and stack at join");
        require(negate(3) == -3);
        require(negate(3L) == -3L);
        require(negate(3F) == -3F);
        require(negate(3D) == -3D);
        require(shiftLeft(3, 2) == 12);
        require(shiftRight(-8, 2) == -2);
        require(shiftUnsigned(-1, 1) == Long.MAX_VALUE);
        require(arrayJoin(true).equals("ok"));
        require(arrayJoin(false).equals(7));
        require(safeDivide(1, 0) == -1);
        require(safeDivide(8, 2) == 4);
        require(select(2) == 13 && select(99) == 14);
        FrameRegression first = new FrameRegression(true);
        FrameRegression second = new FrameRegression(false);
        require(first.count++ == 1 && first.count == 2);
        require(second.count++ == 3 && second.count == 4);
        System.out.println("frames-ok");
    }

    static long wideJoin(long value, boolean flag) { return value + (flag ? 1L : 2L); }
    static double wideJoin(double value, boolean flag) { return value + (flag ? 1.0 : 2.0); }
}
