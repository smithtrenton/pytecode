/**
 * Validation fixture: Java 25 features.
 * Exercises flexible constructor bodies (JEP 513: pre-super() statements)
 * and instance main methods (JEP 512). Produces classfile version 69.
 */
public class Java25Features {

    // Flexible constructor bodies (JEP 513)
    private final int value;
    private final String label;

    public Java25Features(int value) {
        // Pre-super() validation — allowed in Java 25
        if (value < 0) {
            throw new IllegalArgumentException("value must be non-negative: " + value);
        }
        super();
        this.value = value;
        this.label = "item-" + value;
    }

    public Java25Features(String label, int value) {
        // Pre-super() local computation
        int adjusted = Math.abs(value);
        String normalized = label.trim().toLowerCase();
        super();
        this.value = adjusted;
        this.label = normalized;
    }

    public int getValue() {
        return value;
    }

    public String getLabel() {
        return label;
    }

    // Inner class also exercising flexible constructor
    static class Validated {
        private final double amount;

        Validated(double amount) {
            if (Double.isNaN(amount) || Double.isInfinite(amount)) {
                throw new IllegalArgumentException("invalid amount");
            }
            if (amount < 0) {
                throw new IllegalArgumentException("negative amount");
            }
            super();
            this.amount = amount;
        }

        double getAmount() {
            return amount;
        }
    }

    // Instance main method (JEP 512)
    void main() {
        Java25Features f1 = new Java25Features(42);
        System.out.println(f1.getLabel() + " = " + f1.getValue());

        Java25Features f2 = new Java25Features("  Test  ", -7);
        System.out.println(f2.getLabel() + " = " + f2.getValue());

        Validated v = new Validated(99.5);
        System.out.println("amount = " + v.getAmount());
    }
}
