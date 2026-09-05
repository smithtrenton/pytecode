import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;

/** Copy runtime class resources for tests without depending on optional JMOD files. */
public class RuntimeClasses {
    public static void main(String[] args) throws Exception {
        Path root = Paths.get(args[0]);
        for (int i = 1; i < args.length; i++) {
            String resource = args[i] + ".class";
            Path target = root.resolve(resource);
            Files.createDirectories(target.getParent());
            try (InputStream input = ClassLoader.getSystemResourceAsStream(resource)) {
                if (input == null) throw new IllegalArgumentException(resource);
                Files.copy(input, target, StandardCopyOption.REPLACE_EXISTING);
            }
        }
    }
}
