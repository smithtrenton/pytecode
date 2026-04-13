/**
 * Validation fixture: pattern matching (Java 21+).
 * Exercises instanceof patterns and switch patterns with guards.
 */
public class PatternMatching {

    // Sealed hierarchy for exhaustive pattern matching
    sealed interface Expr permits Lit, Add, Mul, Neg {}

    record Lit(int value) implements Expr {}
    record Add(Expr left, Expr right) implements Expr {}
    record Mul(Expr left, Expr right) implements Expr {}
    record Neg(Expr operand) implements Expr {}

    // instanceof pattern matching
    public static String describeObject(Object obj) {
        if (obj instanceof String s && !s.isEmpty()) {
            return "non-empty string: " + s;
        } else if (obj instanceof Integer i && i > 0) {
            return "positive int: " + i;
        } else if (obj instanceof int[] arr && arr.length > 0) {
            return "non-empty int array, first=" + arr[0];
        } else if (obj instanceof Number n) {
            return "number: " + n;
        } else if (obj == null) {
            return "null";
        } else {
            return "other: " + obj.getClass().getSimpleName();
        }
    }

    // Switch pattern matching with guards
    public static int evaluate(Expr expr) {
        return switch (expr) {
            case Lit(int v) -> v;
            case Add(var l, var r) -> evaluate(l) + evaluate(r);
            case Mul(Lit(int a), Lit(int b)) when a == 0 || b == 0 -> 0;
            case Mul(var l, var r) -> evaluate(l) * evaluate(r);
            case Neg(Lit(int v)) when v == 0 -> 0;
            case Neg(var e) -> -evaluate(e);
        };
    }

    // Switch with null handling and default
    public static String classify(Object obj) {
        return switch (obj) {
            case null -> "null";
            case Integer i when i < 0 -> "negative";
            case Integer i when i == 0 -> "zero";
            case Integer i -> "positive: " + i;
            case String s when s.isBlank() -> "blank string";
            case String s -> "string: " + s;
            case Double d -> "double: " + d;
            default -> "unknown";
        };
    }

    // Guarded patterns with complex conditions
    public static String classifyNumber(Number n) {
        return switch (n) {
            case Integer i when i >= 0 && i <= 9 -> "single digit";
            case Integer i when i >= 10 && i <= 99 -> "double digit";
            case Integer i -> "big integer: " + i;
            case Long l when l > Integer.MAX_VALUE -> "beyond int range";
            case Long l -> "fits in int: " + l;
            case Double d when Double.isNaN(d) -> "NaN";
            case Double d when Double.isInfinite(d) -> "infinite";
            case Double d -> "finite double: " + d;
            default -> "other number type";
        };
    }

    public static void main(String[] args) {
        // instanceof patterns
        System.out.println(describeObject("hello"));
        System.out.println(describeObject(42));
        System.out.println(describeObject(3.14));
        System.out.println(describeObject(null));
        System.out.println(describeObject(new int[]{1, 2, 3}));

        // Expression evaluation via pattern matching
        Expr expr = new Add(new Lit(3), new Mul(new Lit(2), new Lit(4)));
        System.out.println("3 + 2*4 = " + evaluate(expr));

        Expr zero = new Mul(new Lit(0), new Lit(999));
        System.out.println("0 * 999 = " + evaluate(zero));

        // Switch classification
        System.out.println(classify(null));
        System.out.println(classify(-5));
        System.out.println(classify(0));
        System.out.println(classify(42));
        System.out.println(classify("hello"));
        System.out.println(classify("  "));
        System.out.println(classify(3.14));

        // Number classification
        System.out.println(classifyNumber(7));
        System.out.println(classifyNumber(42));
        System.out.println(classifyNumber(1000));
        System.out.println(classifyNumber(Double.NaN));
    }
}
