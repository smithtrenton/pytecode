public class StaticInit {
    public static int counter;
    private final int value;

    static {
        counter = 42;
    }

    {
        value = counter + 1;
    }

    public StaticInit() {
    }
}
