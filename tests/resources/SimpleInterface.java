public interface SimpleInterface {
    int CONSTANT_VALUE = 42;

    void abstractMethod();

    String anotherAbstract(int x);

    default String defaultMethod() {
        return "default";
    }
}
