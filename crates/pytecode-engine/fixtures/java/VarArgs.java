/**
 * Validation fixture: variable-argument methods.
 * Exercises ACC_VARARGS flag and array parameter descriptors.
 */
public class VarArgs {

    public static int sum(int... values) {
        int total = 0;
        for (int v : values) {
            total += v;
        }
        return total;
    }

    public static String join(String separator, Object... parts) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < parts.length; i++) {
            if (i > 0) sb.append(separator);
            sb.append(parts[i]);
        }
        return sb.toString();
    }

    @SafeVarargs
    public static <T> T firstNonNull(T... values) {
        for (T v : values) {
            if (v != null) return v;
        }
        throw new IllegalArgumentException("all null");
    }

    public static void main(String[] args) {
        System.out.println(sum(1, 2, 3));
        System.out.println(join(", ", "a", "b", "c"));
        System.out.println(firstNonNull(null, "hello", "world"));
    }
}
