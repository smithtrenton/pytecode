/**
 * Validation fixture: nest-based access control (Java 11+).
 * Exercises NestHost and NestMembers attributes, and private member access
 * across nest mates without synthetic bridge methods.
 */
public class NestAccess {

    private int outerField = 42;

    private static String outerStaticField = "outer";

    private int getOuterField() {
        return outerField;
    }

    private static String getOuterStatic() {
        return outerStaticField;
    }

    class Inner {
        private int innerField = 100;

        private int getInnerField() {
            return innerField;
        }

        // Access outer private members (nest-based access, no bridge in Java 11+)
        public int accessOuter() {
            return outerField + getOuterField();
        }

        public String accessOuterStatic() {
            return outerStaticField + getOuterStatic();
        }
    }

    static class StaticNested {
        private String nestedField = "nested";

        private String getNestedField() {
            return nestedField;
        }

        // Access outer private static members
        public String accessOuterStatic() {
            return outerStaticField + getOuterStatic();
        }
    }

    // Outer accessing inner private members (nest-based access)
    public int accessInnerPrivate() {
        Inner inner = new Inner();
        return inner.innerField + inner.getInnerField();
    }

    public String accessNestedPrivate() {
        StaticNested nested = new StaticNested();
        return nested.nestedField + nested.getNestedField();
    }

    // Anonymous class also part of the nest
    public Runnable createAnonymous() {
        return new Runnable() {
            @Override
            public void run() {
                System.out.println(outerField);
                System.out.println(getOuterField());
            }
        };
    }

    public static void main(String[] args) {
        NestAccess outer = new NestAccess();
        Inner inner = outer.new Inner();
        System.out.println(inner.accessOuter());
        System.out.println(inner.accessOuterStatic());
        System.out.println(outer.accessInnerPrivate());
        System.out.println(outer.accessNestedPrivate());
        outer.createAnonymous().run();
    }
}
