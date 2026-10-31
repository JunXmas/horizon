import unittest

from tools import remap


class ProguardToTinyTests(unittest.TestCase):
    def test_fields_methods_and_arrays_use_official_descriptors(self):
        text = ("# comment\nnet.minecraft.Foo -> a:\n    int count -> b\n"
                "    1:2:net.minecraft.Foo[] all(int,java.lang.String) -> c\n")
        self.assertEqual(remap.proguard_to_tiny(text),
                         "v1\tofficial\tnamed\nCLASS\ta\tnet/minecraft/Foo\nFIELD\ta\tI\tb\tcount\n"
                         "METHOD\ta\t(ILjava/lang/String;)[La;\tc\tall\n")

    def test_line_ranges_once_and_inlined_frames_dropped(self):
        text = ("net.minecraft.Foo -> a:\n    1:2:void run() -> c\n    3:4:void run() -> c\n"
                "    5:6:void net.minecraft.Bar.inlined():7:8 -> c\n")
        self.assertEqual(remap.proguard_to_tiny(text).count("METHOD"), 1)

    def test_unreadable_line_is_refused(self):
        with self.assertRaises(ValueError):
            remap.proguard_to_tiny("net.minecraft.Foo -> a:\n    garbage\n")


if __name__ == "__main__":
    unittest.main()
