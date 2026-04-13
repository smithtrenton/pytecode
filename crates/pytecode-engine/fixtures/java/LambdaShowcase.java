/**
 * Validation fixture: lambda expressions and functional interfaces.
 * Exercises capturing lambdas, effectively-final variables, multi-arg functional
 * interfaces, and BootstrapMethods attribute patterns.
 */
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.List;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Predicate;

public class LambdaShowcase {

    @FunctionalInterface
    interface TriFunction<A, B, C, R> {
        R apply(A a, B b, C c);
    }

    // Non-capturing lambda (stateless)
    private static final Predicate<String> IS_EMPTY = s -> s.isEmpty();

    // Capturing lambda (captures instance field)
    private final String prefix;

    public LambdaShowcase(String prefix) {
        this.prefix = prefix;
    }

    public Function<String, String> prefixer() {
        return s -> prefix + s;
    }

    // Lambda capturing local variable (effectively final)
    public static Predicate<String> longerThan(int minLength) {
        return s -> s.length() > minLength;
    }

    // Multi-arg lambda
    public static <T> List<T> zipWith(
            List<T> a, List<T> b, BiFunction<T, T, T> combiner) {
        List<T> result = new ArrayList<>();
        int size = Math.min(a.size(), b.size());
        for (int i = 0; i < size; i++) {
            result.add(combiner.apply(a.get(i), b.get(i)));
        }
        return result;
    }

    // Custom tri-function lambda
    public static <A, B, C, R> R applyTri(
            A a, B b, C c, TriFunction<A, B, C, R> fn) {
        return fn.apply(a, b, c);
    }

    // Lambda used in sorting
    public static void sortByLength(List<String> list) {
        list.sort(Comparator.comparingInt(String::length)
            .thenComparing(Comparator.naturalOrder()));
    }

    // Lambda with exception handling pattern
    public static <T> Consumer<T> quiet(ThrowingConsumer<T> consumer) {
        return t -> {
            try {
                consumer.accept(t);
            } catch (Exception e) {
                throw new RuntimeException(e);
            }
        };
    }

    @FunctionalInterface
    interface ThrowingConsumer<T> {
        void accept(T t) throws Exception;
    }

    public static void main(String[] args) {
        // Non-capturing
        System.out.println(IS_EMPTY.test(""));
        System.out.println(IS_EMPTY.test("x"));

        // Capturing instance field
        LambdaShowcase showcase = new LambdaShowcase(">> ");
        Function<String, String> fn = showcase.prefixer();
        System.out.println(fn.apply("hello"));

        // Capturing local
        Predicate<String> p = longerThan(3);
        System.out.println(p.test("hi"));
        System.out.println(p.test("hello"));

        // Zip
        List<String> result = zipWith(
            Arrays.asList("a", "b"), Arrays.asList("1", "2"),
            (x, y) -> x + y);
        System.out.println(result);

        // Tri-function
        String triResult = applyTri("hello", " ", "world",
            (a, b, c) -> a + b + c);
        System.out.println(triResult);

        // Sort
        List<String> words = new ArrayList<>(Arrays.asList("banana", "fig", "apple", "date"));
        sortByLength(words);
        System.out.println(words);

        // Quiet consumer
        List<String> items = Arrays.asList("1", "2", "3");
        items.forEach(quiet(s -> System.out.println(Integer.parseInt(s))));
    }
}
