import java.io.Serializable;

public class MultiInterface implements Serializable, Comparable<MultiInterface> {
    private final int id;

    public MultiInterface(int id) {
        this.id = id;
    }

    @Override
    public int compareTo(MultiInterface other) {
        return Integer.compare(this.id, other.id);
    }
}
