/**
 * Validation fixture: static and default methods in interfaces.
 * Exercises INVOKESTATIC on interface types and ACC_PRIVATE on interface methods (Java 9+).
 */
public class StaticInterfaceMethods {

    interface Validator<T> {
        boolean validate(T value);

        default Validator<T> and(Validator<T> other) {
            return value -> this.validate(value) && other.validate(value);
        }

        default Validator<T> or(Validator<T> other) {
            return value -> this.validate(value) || other.validate(value);
        }

        default Validator<T> negate() {
            return value -> !this.validate(value);
        }

        static <T> Validator<T> alwaysTrue() {
            return value -> true;
        }

        static <T> Validator<T> alwaysFalse() {
            return value -> false;
        }

        static Validator<String> nonEmpty() {
            return s -> s != null && !s.isEmpty();
        }
    }

    interface Transformer<T, R> {
        R transform(T input);

        default <V> Transformer<T, V> andThen(Transformer<R, V> after) {
            return input -> after.transform(this.transform(input));
        }

        static <T> Transformer<T, T> identity() {
            return input -> input;
        }
    }

    public static void main(String[] args) {
        Validator<String> nonEmpty = Validator.nonEmpty();
        Validator<String> longerThan3 = s -> s.length() > 3;
        Validator<String> combined = nonEmpty.and(longerThan3);

        System.out.println(combined.validate("hello"));
        System.out.println(combined.validate("hi"));
        System.out.println(combined.negate().validate("hi"));

        Transformer<String, Integer> length = String::length;
        Transformer<String, String> lengthStr = length.andThen(Object::toString);
        System.out.println(lengthStr.transform("hello"));

        Transformer<String, String> id = Transformer.identity();
        System.out.println(id.transform("unchanged"));
    }
}
