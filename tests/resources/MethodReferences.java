/**
 * Validation fixture: method references (Java 8+).
 * Exercises constructor refs, static refs, instance refs, and additional
 * invokedynamic/bootstrap method patterns beyond basic lambdas.
 */
import java.util.Arrays;
import java.util.List;
import java.util.function.Function;
import java.util.function.Supplier;
import java.util.function.UnaryOperator;

public class MethodReferences {

    private final String name;

    public MethodReferences(String name) {
        this.name = name;
    }

    public String getName() {
        return name;
    }

    public static String toUpperCase(String s) {
        return s.toUpperCase();
    }

    public int nameLength() {
        return name.length();
    }

    public static void main(String[] args) {
        // Constructor reference
        Function<String, MethodReferences> ctor = MethodReferences::new;
        MethodReferences obj = ctor.apply("test");

        // Static method reference
        UnaryOperator<String> upper = MethodReferences::toUpperCase;
        System.out.println(upper.apply("hello"));

        // Instance method reference (bound)
        Supplier<String> getter = obj::getName;
        System.out.println(getter.get());

        // Instance method reference (unbound)
        Function<String, String> trim = String::trim;
        System.out.println(trim.apply("  padded  "));

        // Method reference in stream pipeline
        List<String> names = Arrays.asList("alice", "bob", "charlie");
        names.stream()
            .map(MethodReferences::toUpperCase)
            .forEach(System.out::println);
    }
}
