public class TryCatchExample {
    public int safeDivide(int a, int b) {
        try {
            return a / b;
        } catch (ArithmeticException e) {
            return -1;
        } finally {
            System.out.println("done");
        }
    }

    public void multiCatch() {
        try {
            Object o = null;
            o.toString();
        } catch (NullPointerException e) {
            System.err.println("null");
        } catch (RuntimeException e) {
            System.err.println("runtime");
        }
    }
}
