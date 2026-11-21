public class LdcSigns
{
	// Values chosen so javac must emit ldc/ldc_w/ldc2_w rather than
	// iconst/bipush/sipush: forces integer and long constants through the
	// constant pool where their signed 32/64-bit encodings are observable.
	public int negativeInt()
	{
		return -484534915;
	}

	public long negativeLong()
	{
		return -0x1234567890L;
	}

	public int maxInt()
	{
		return 2147483647;
	}

	public long minLong()
	{
		return Long.MIN_VALUE;
	}
}
