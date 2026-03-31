/**
 * Validation fixture: complex generic signatures.
 * Exercises wildcards, recursive bounds, multi-bounds, and complex Signature attributes.
 */
import java.io.Serializable;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.List;

public class ComplexGenerics {

    // Recursive bound: T extends Comparable<T>
    public static <T extends Comparable<T>> T max(T a, T b) {
        return a.compareTo(b) >= 0 ? a : b;
    }

    // Multi-bound: T extends Number & Comparable<T>
    public static <T extends Number & Comparable<T>> T clamp(T value, T min, T max) {
        if (value.compareTo(min) < 0) return min;
        if (value.compareTo(max) > 0) return max;
        return value;
    }

    // Wildcard extends
    public static double sumList(List<? extends Number> list) {
        double sum = 0;
        for (Number n : list) {
            sum += n.doubleValue();
        }
        return sum;
    }

    // Wildcard super
    public static void addIntegers(List<? super Integer> list, int count) {
        for (int i = 0; i < count; i++) {
            list.add(i);
        }
    }

    // Complex nested generics
    public static <K extends Comparable<K>, V> List<V> sortedValues(
            List<? extends java.util.Map.Entry<K, V>> entries) {
        entries.sort(Comparator.comparing(java.util.Map.Entry::getKey));
        List<V> result = new ArrayList<>();
        for (java.util.Map.Entry<K, V> entry : entries) {
            result.add(entry.getValue());
        }
        return result;
    }

    // Self-referential generic type
    static abstract class Builder<B extends Builder<B>> {
        protected String name;

        @SuppressWarnings("unchecked")
        public B withName(String name) {
            this.name = name;
            return (B) this;
        }
    }

    static class ConcreteBuilder extends Builder<ConcreteBuilder> {
        private int count;

        public ConcreteBuilder withCount(int count) {
            this.count = count;
            return this;
        }

        @Override
        public String toString() {
            return name + ":" + count;
        }
    }

    // Generic method with multiple type parameters and bounds
    public static <T extends Serializable & Comparable<T>, C extends Collection<T>>
            C filterAndSort(C collection, T threshold) {
        collection.removeIf(item -> item.compareTo(threshold) < 0);
        return collection;
    }

    public static void main(String[] args) {
        System.out.println(max("apple", "banana"));
        System.out.println(clamp(5, 1, 10));
        List<Number> nums = new ArrayList<>();
        nums.add(1);
        nums.add(2.5);
        nums.add(3);
        System.out.println(sumList(nums));

        List<Object> objects = new ArrayList<>();
        addIntegers(objects, 3);
        System.out.println(objects);

        ConcreteBuilder builder = new ConcreteBuilder().withName("test").withCount(42);
        System.out.println(builder);
    }
}
