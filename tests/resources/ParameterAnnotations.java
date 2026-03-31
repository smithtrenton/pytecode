import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.PARAMETER)
@interface VisibleParam {
}

@Retention(RetentionPolicy.CLASS)
@Target(ElementType.PARAMETER)
@interface InvisibleParam {
}

public class ParameterAnnotations {
    public void annotated(@VisibleParam String text, @InvisibleParam int count) {
    }
}
