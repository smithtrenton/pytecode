import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
import java.util.List;

@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE_USE, ElementType.TYPE_PARAMETER})
@interface VisibleTypeUse {
}

@Retention(RetentionPolicy.CLASS)
@Target({ElementType.TYPE_USE, ElementType.TYPE_PARAMETER})
@interface InvisibleTypeUse {
}

public class TypeAnnotationShowcase<@VisibleTypeUse @InvisibleTypeUse T extends @VisibleTypeUse Number> {
    private final List<@VisibleTypeUse @InvisibleTypeUse String> field;

    public TypeAnnotationShowcase(List<@VisibleTypeUse @InvisibleTypeUse String> field)
            throws @VisibleTypeUse Exception {
        this.field = field;
    }

    public @VisibleTypeUse @InvisibleTypeUse String method(
            List<@VisibleTypeUse @InvisibleTypeUse String> input) {
        @VisibleTypeUse @InvisibleTypeUse String local = input.get(0);
        return (@VisibleTypeUse @InvisibleTypeUse String) local;
    }
}
