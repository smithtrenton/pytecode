package fixture.hierarchy;

public class HierarchyFixture extends Mammal implements Pet {
    public void train() {}

    void packageHook() {}

    protected void protectedHook() {}
}

class Mammal extends Animal implements Trainable {
    public void train() {}

    void packageHook() {}

    protected void protectedHook() {}
}

class Animal {
    public void eat() {}
}

interface Pet {}

interface Trainable {
    void train();
}
