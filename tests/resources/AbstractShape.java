public abstract class AbstractShape {
    private final String name;
    protected double area;

    public AbstractShape(String name) {
        this.name = name;
    }

    public String getName() {
        return name;
    }

    public abstract double computeArea();

    public abstract double computePerimeter();
}
