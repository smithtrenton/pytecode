/**
 * Validation fixture: records (Java 16+).
 * Exercises Record attribute, RecordComponent attributes, canonical constructor,
 * implicit accessor methods, generated equals/hashCode/toString.
 */
import java.util.List;
import java.util.Objects;

public class RecordClass {

    // Simple record
    public record Point(int x, int y) {}

    // Record with custom canonical constructor
    public record Range(int start, int end) {
        public Range {
            if (start > end) {
                throw new IllegalArgumentException("start > end");
            }
        }
    }

    // Record with custom methods
    public record NamedValue<T>(String name, T value) {
        public String display() {
            return name + "=" + value;
        }

        public boolean hasValue() {
            return value != null;
        }
    }

    // Record implementing interface
    public interface Measurable {
        double measure();
    }

    public record Circle(double radius) implements Measurable {
        @Override
        public double measure() {
            return Math.PI * radius * radius;
        }
    }

    // Record with static members
    public record Pair<A, B>(A first, B second) {
        public static <T> Pair<T, T> of(T value) {
            return new Pair<>(value, value);
        }

        public Pair<B, A> swap() {
            return new Pair<>(second, first);
        }
    }

    // Nested record
    public record Line(Point start, Point end) {
        public double length() {
            int dx = end.x() - start.x();
            int dy = end.y() - start.y();
            return Math.sqrt(dx * dx + dy * dy);
        }
    }

    // Record with annotation on component
    public record Validated(@Deprecated String oldName, String newName) {
        public String effectiveName() {
            return newName != null ? newName : oldName;
        }
    }

    public static void main(String[] args) {
        Point p = new Point(1, 2);
        System.out.println(p);
        System.out.println(p.x() + ", " + p.y());

        Range r = new Range(0, 10);
        System.out.println(r);

        NamedValue<Integer> nv = new NamedValue<>("count", 42);
        System.out.println(nv.display());

        Circle c = new Circle(5.0);
        System.out.println(c.measure());

        Pair<String, Integer> pair = new Pair<>("key", 1);
        System.out.println(pair.swap());

        Line line = new Line(new Point(0, 0), new Point(3, 4));
        System.out.println(line.length());

        Validated v = new Validated("old", "new");
        System.out.println(v.effectiveName());
    }
}
