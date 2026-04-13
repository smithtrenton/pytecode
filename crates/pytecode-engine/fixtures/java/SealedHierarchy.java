/**
 * Validation fixture: sealed classes and interfaces (Java 17+).
 * Exercises PermittedSubclasses attribute, sealed/non-sealed modifiers.
 */
public class SealedHierarchy {

    // Sealed interface with permitted implementations
    public sealed interface Shape permits Circle, Rectangle, Triangle {
        double area();
        String name();
    }

    // Final implementation (cannot be extended)
    public record Circle(double radius) implements Shape {
        @Override
        public double area() {
            return Math.PI * radius * radius;
        }

        @Override
        public String name() {
            return "Circle(r=" + radius + ")";
        }
    }

    // Sealed implementation (can only be extended by listed classes)
    public sealed static abstract class Rectangle implements Shape permits Square, FilledRectangle {
        protected final double width;
        protected final double height;

        public Rectangle(double width, double height) {
            this.width = width;
            this.height = height;
        }

        @Override
        public double area() {
            return width * height;
        }

        @Override
        public String name() {
            return "Rectangle(" + width + "x" + height + ")";
        }
    }

    // Final subclass of sealed class
    public static final class Square extends Rectangle {
        public Square(double side) {
            super(side, side);
        }

        @Override
        public String name() {
            return "Square(" + width + ")";
        }
    }

    // Non-sealed subclass (opens hierarchy back up)
    public static non-sealed class FilledRectangle extends Rectangle {
        private final String color;

        public FilledRectangle(double w, double h, String color) {
            super(w, h);
            this.color = color;
        }

        @Override
        public String name() {
            return "FilledRectangle(" + width + "x" + height + ", " + color + ")";
        }
    }

    // Non-sealed record (also a valid permit target)
    public record Triangle(double base, double height) implements Shape {
        @Override
        public double area() {
            return 0.5 * base * height;
        }

        @Override
        public String name() {
            return "Triangle(b=" + base + ", h=" + height + ")";
        }
    }

    // Extension of non-sealed class is allowed
    public static class BorderedRectangle extends FilledRectangle {
        private final int borderWidth;

        public BorderedRectangle(double w, double h, String color, int borderWidth) {
            super(w, h, color);
            this.borderWidth = borderWidth;
        }

        @Override
        public String name() {
            return super.name() + " border=" + borderWidth;
        }
    }

    // Using sealed hierarchy in methods
    public static String describe(Shape shape) {
        return shape.name() + " area=" + shape.area();
    }

    public static void main(String[] args) {
        Shape[] shapes = {
            new Circle(5),
            new Square(4),
            new FilledRectangle(3, 6, "red"),
            new Triangle(8, 3),
            new BorderedRectangle(2, 4, "blue", 1)
        };

        for (Shape s : shapes) {
            System.out.println(describe(s));
        }
    }
}
