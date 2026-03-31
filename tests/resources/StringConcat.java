/**
 * Validation fixture: string concatenation via invokedynamic (Java 9+).
 * At --release 9+, javac generates invokedynamic with StringConcatFactory
 * instead of explicit StringBuilder chains.
 */
public class StringConcat {

    private final String name;
    private final int age;

    public StringConcat(String name, int age) {
        this.name = name;
        this.age = age;
    }

    // Simple concatenation (two strings)
    public static String greet(String name) {
        return "Hello, " + name + "!";
    }

    // Mixed types
    public static String describe(String item, int count, double price) {
        return item + " x" + count + " @ $" + price;
    }

    // Concatenation with boolean and char
    public static String flags(boolean active, char grade) {
        return "active=" + active + ", grade=" + grade;
    }

    // Instance field concatenation
    public String toDisplay() {
        return name + " (age " + age + ")";
    }

    // Concatenation in a loop (still uses invokedynamic per iteration)
    public static String repeat(String s, int times) {
        String result = "";
        for (int i = 0; i < times; i++) {
            result = result + s;
        }
        return result;
    }

    // Null handling in concatenation
    public static String withNull(Object obj) {
        return "value=" + obj;
    }

    // Long and float concatenation
    public static String numbers(long l, float f) {
        return "long=" + l + ", float=" + f;
    }

    public static void main(String[] args) {
        System.out.println(greet("World"));
        System.out.println(describe("Widget", 5, 9.99));
        System.out.println(flags(true, 'A'));
        System.out.println(new StringConcat("Alice", 30).toDisplay());
        System.out.println(repeat("ab", 3));
        System.out.println(withNull(null));
        System.out.println(numbers(123456789L, 3.14f));
    }
}
