/**
 * Validation fixture: enum with abstract methods and constant-specific behavior.
 * Exercises anonymous inner class generation for enum constants and InnerClasses attribute.
 */
public enum EnumAdvanced {
    CIRCLE {
        @Override
        public double area(double size) {
            return Math.PI * size * size;
        }

        @Override
        public String label() {
            return "Circle";
        }
    },
    SQUARE {
        @Override
        public double area(double size) {
            return size * size;
        }

        @Override
        public String label() {
            return "Square";
        }
    },
    TRIANGLE {
        @Override
        public double area(double size) {
            return 0.5 * size * size;
        }

        @Override
        public String label() {
            return "Triangle";
        }
    };

    public abstract double area(double size);

    public abstract String label();

    public String describe(double size) {
        return label() + " with area " + area(size);
    }
}
