public class CfgEdgeCaseFixture {

    static final class TrackedResource implements AutoCloseable {
        private final int value;

        TrackedResource(int value) {
            this.value = value;
        }

        int value() {
            return value;
        }

        @Override
        public void close() throws Exception {
            if (value < 0) {
                throw new Exception("close");
            }
        }
    }

    public int overlappingHandlers(int x) {
        try {
            if (x == 0) {
                throw new NullPointerException();
            }
            return 100 / x;
        } catch (NullPointerException e) {
            return -1;
        } catch (RuntimeException e) {
            return -2;
        }
    }

    public int catchAllHandler(int x) {
        try {
            return 100 / x;
        } finally {
            x++;
        }
    }

    public String multiCatch(Object obj) {
        try {
            if (obj == null) {
                throw new NullPointerException();
            }
            if (!(obj instanceof String)) {
                throw new IllegalArgumentException();
            }
            return ((String) obj).trim();
        } catch (NullPointerException | IllegalArgumentException e) {
            return "fallback";
        }
    }

    public int synchronizedBlock(Object lock) {
        synchronized (lock) {
            return lock.hashCode();
        }
    }

    public int unreachableAfterGoto(int x) {
        int result = x;
        label: {
            if (x > 0) {
                result = x + 1;
                break label;
            }
            result = x - 1;
        }
        return result;
    }

    public int unreachableAfterReturn(int x) {
        if (x < 0) {
            return -1;
        }
        return x + 1;
    }

    public Object constructorInBranch(boolean flag) {
        if (flag) {
            return new StringBuilder("left");
        }
        return new StringBuilder("right");
    }

    public int largeTableSwitch(int x) {
        switch (x) {
            case 0: return 0;
            case 1: return 1;
            case 2: return 2;
            case 3: return 3;
            case 4: return 4;
            case 5: return 5;
            case 6: return 6;
            case 7: return 7;
            case 8: return 8;
            case 9: return 9;
            case 10: return 10;
            case 11: return 11;
            default: return -1;
        }
    }

    public int largeLookupSwitch(int x) {
        switch (x) {
            case -1000: return 1;
            case -100: return 2;
            case -10: return 3;
            case -1: return 4;
            case 0: return 5;
            case 1: return 6;
            case 10: return 7;
            case 100: return 8;
            case 1000: return 9;
            case 10000: return 10;
            case 100000: return 11;
            default: return -1;
        }
    }

    public int handlerThatThrows(int a, int b) {
        try {
            return a / b;
        } catch (ArithmeticException e) {
            throw new IllegalStateException(e);
        }
    }

    public int tryWithResources(int value) throws Exception {
        try (TrackedResource resource = new TrackedResource(value)) {
            return resource.value();
        }
    }

    public int deeplyNestedTryCatch(int a, int b, int c, int d) {
        try {
            try {
                try {
                    return a / b;
                } catch (ArithmeticException e1) {
                    return a / c;
                }
            } catch (ArithmeticException e2) {
                return a / d;
            }
        } catch (ArithmeticException e3) {
            return 0;
        }
    }

    public int loopWithHandler(int[] arr) {
        int total = 0;
        for (int i = 0; i < arr.length; i++) {
            try {
                total += 100 / arr[i];
            } catch (ArithmeticException e) {
                total--;
            }
        }
        return total;
    }

    public int switchFallThrough(int x) {
        switch (x) {
            case 0:
            case 1:
                x += 10;
                break;
            case 2:
                x += 20;
            default:
                x += 30;
        }
        return x;
    }
}
