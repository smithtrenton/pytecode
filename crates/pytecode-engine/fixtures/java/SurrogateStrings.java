import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

@Retention(RetentionPolicy.RUNTIME)
@interface SurrogateMarker { String value() default "\uD800"; }

@SurrogateMarker("\uDC00")
public class SurrogateStrings {
    public static final String CONSTANT = "\uD800";
    public static String high() { return "\uD800"; }
    public static String low() { return "\uDC00"; }
    public static String mixed() { return "A\u0000\uD800B\uDC00\uD83D\uDE00"; }
    public static void main(String[] args) {
        if (high().length() != 1 || high().charAt(0) != 0xD800) throw new AssertionError("high");
        if (low().length() != 1 || low().charAt(0) != 0xDC00) throw new AssertionError("low");
        if (mixed().length() != 7) throw new AssertionError("mixed");
        if (SurrogateStrings.class.getAnnotation(SurrogateMarker.class).value().charAt(0) != 0xDC00)
            throw new AssertionError("annotation");
        System.out.println("surrogates-ok");
    }
}
