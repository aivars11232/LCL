import com.google.zxing.BinaryBitmap;
import com.google.zxing.DecodeHintType;
import com.google.zxing.RGBLuminanceSource;
import com.google.zxing.common.HybridBinarizer;
import com.google.zxing.qrcode.QRCodeReader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.Map;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Reads the QR code `lcl-remote pair --json` draws, with ZXing — the library
 * the app's scanner is built on — and prints what it says. tools/e2e.sh checks
 * that this is exactly the pairing link: the PC's QR code is one the phone can
 * read.
 *
 *   java -cp zxing-core.jar QrDecode.java code.svg
 *
 * The SVG is one path of unit squares, `M x y h1v1h-1z`, on a white square
 * `viewBox="0 0 size size"`; it is drawn here at six pixels a module.
 */
public class QrDecode {
    public static void main(String[] args) throws Exception {
        String svg = Files.readString(Path.of(args[0]));
        Matcher box = Pattern.compile("viewBox=\"0 0 (\\d+) (\\d+)\"").matcher(svg);
        if (!box.find()) throw new IllegalArgumentException("no viewBox in " + args[0]);
        int modules = Integer.parseInt(box.group(1));
        int scale = 6;
        int side = modules * scale;
        int[] pixels = new int[side * side];
        Arrays.fill(pixels, 0xFFFFFFFF);
        Matcher square = Pattern.compile("M(\\d+) (\\d+)h1v1h-1z").matcher(svg);
        int dark = 0;
        while (square.find()) {
            int x = Integer.parseInt(square.group(1));
            int y = Integer.parseInt(square.group(2));
            for (int dy = 0; dy < scale; dy++) {
                Arrays.fill(pixels, (y * scale + dy) * side + x * scale, (y * scale + dy) * side + (x + 1) * scale, 0xFF000000);
            }
            dark++;
        }
        if (dark == 0) throw new IllegalArgumentException("no dark modules in " + args[0]);
        BinaryBitmap bitmap = new BinaryBitmap(new HybridBinarizer(new RGBLuminanceSource(side, side, pixels)));
        String text = new QRCodeReader().decode(bitmap, Map.of(DecodeHintType.PURE_BARCODE, Boolean.TRUE)).getText();
        System.out.println(text);
    }
}
