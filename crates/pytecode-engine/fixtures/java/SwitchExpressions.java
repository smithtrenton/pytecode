/**
 * Validation fixture: switch expressions (Java 14+).
 * Exercises arrow-syntax switch, yield, expression-form switch, and exhaustive
 * enum/sealed switch.
 */
public class SwitchExpressions {

    enum Day { MONDAY, TUESDAY, WEDNESDAY, THURSDAY, FRIDAY, SATURDAY, SUNDAY }

    // Arrow-syntax switch expression
    public static String dayType(Day day) {
        return switch (day) {
            case MONDAY, TUESDAY, WEDNESDAY, THURSDAY, FRIDAY -> "Weekday";
            case SATURDAY, SUNDAY -> "Weekend";
        };
    }

    // Switch expression with yield
    public static int dayNumber(Day day) {
        return switch (day) {
            case MONDAY -> 1;
            case TUESDAY -> 2;
            case WEDNESDAY -> 3;
            case THURSDAY -> 4;
            case FRIDAY -> 5;
            case SATURDAY -> {
                System.out.println("Almost done!");
                yield 6;
            }
            case SUNDAY -> {
                System.out.println("Rest day!");
                yield 7;
            }
        };
    }

    // Switch expression over int
    public static String classify(int value) {
        return switch (value) {
            case 0 -> "zero";
            case 1, 2, 3 -> "small";
            case 4, 5, 6, 7, 8, 9 -> "medium";
            default -> {
                if (value < 0) {
                    yield "negative";
                } else {
                    yield "large";
                }
            }
        };
    }

    // Switch expression over String
    public static int parseMonth(String month) {
        return switch (month.toLowerCase()) {
            case "january", "jan" -> 1;
            case "february", "feb" -> 2;
            case "march", "mar" -> 3;
            case "april", "apr" -> 4;
            case "may" -> 5;
            case "june", "jun" -> 6;
            case "july", "jul" -> 7;
            case "august", "aug" -> 8;
            case "september", "sep" -> 9;
            case "october", "oct" -> 10;
            case "november", "nov" -> 11;
            case "december", "dec" -> 12;
            default -> throw new IllegalArgumentException("Unknown: " + month);
        };
    }

    // Nested switch expressions
    public static String matrix(int row, int col) {
        return switch (row) {
            case 0 -> switch (col) {
                case 0 -> "origin";
                case 1 -> "right";
                default -> "far right";
            };
            case 1 -> switch (col) {
                case 0 -> "down";
                default -> "diagonal";
            };
            default -> "deep";
        };
    }

    public static void main(String[] args) {
        for (Day day : Day.values()) {
            System.out.println(day + ": " + dayType(day) + " (" + dayNumber(day) + ")");
        }
        System.out.println(classify(-5));
        System.out.println(classify(0));
        System.out.println(classify(3));
        System.out.println(classify(100));
        System.out.println(parseMonth("March"));
        System.out.println(matrix(0, 1));
    }
}
