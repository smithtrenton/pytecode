import java.lang.annotation.*;

@Deprecated
public class AnnotatedClass {
    @Deprecated
    public int oldField;

    @Deprecated
    public void oldMethod() {
    }

    @SuppressWarnings("unchecked")
    public void suppressedMethod() {
    }
}
