public class ControlFlowExample {
    public int branch(boolean flag, int x) {
        if (flag) {
            return x + 1;
        }
        return x - 1;
    }

    public int loopSum(int limit) {
        int total = 0;
        for (int i = 0; i < limit; i++) {
            total += i;
        }
        return total;
    }

    public int denseSwitch(int value) {
        switch (value) {
            case 0:
                return 10;
            case 1:
                return 11;
            case 2:
                return 12;
            default:
                return -1;
        }
    }

    public int sparseSwitch(int value) {
        switch (value) {
            case -100:
                return 1;
            case 0:
                return 2;
            case 1000:
                return 3;
            default:
                return -1;
        }
    }
}
