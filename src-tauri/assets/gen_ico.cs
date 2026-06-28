using System;
using System.Drawing;
using System.IO;

class IcoWriter {
    static void Main(string[] args) {
        string srcPath = args[0];
        string outPath = args[1];
        int[] sizes = {16, 32, 48, 64, 128, 256};

        Image src = Image.FromFile(srcPath);
        MemoryStream ms = new MemoryStream();
        BinaryWriter writer = new BinaryWriter(ms);

        // ICO header
        writer.Write((short)0);
        writer.Write((short)1);
        writer.Write((short)sizes.Length);

        int offset = 6 + sizes.Length * 16;
        byte[][] blobs = new byte[sizes.Length][];

        for (int i = 0; i < sizes.Length; i++) {
            Bitmap bmp = new Bitmap(sizes[i], sizes[i]);
            Graphics g = Graphics.FromImage(bmp);
            g.InterpolationMode = System.Drawing.Drawing2D.InterpolationMode.HighQualityBicubic;
            g.DrawImage(src, 0, 0, sizes[i], sizes[i]);
            g.Dispose();
            MemoryStream blob = new MemoryStream();
            bmp.Save(blob, System.Drawing.Imaging.ImageFormat.Png);
            blobs[i] = blob.ToArray();
            bmp.Dispose();
        }

        for (int i = 0; i < sizes.Length; i++) {
            int s = sizes[i] == 256 ? 0 : sizes[i];
            writer.Write((byte)s);
            writer.Write((byte)s);
            writer.Write((byte)0);
            writer.Write((byte)0);
            writer.Write((short)1);
            writer.Write((short)32);
            writer.Write(blobs[i].Length);
            writer.Write(offset);
            offset += blobs[i].Length;
        }
        foreach (byte[] blob in blobs) writer.Write(blob);
        src.Dispose();
        File.WriteAllBytes(outPath, ms.ToArray());
        Console.WriteLine("ICO généré : " + outPath);
    }
}
