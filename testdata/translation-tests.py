import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

BASE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('translation', BASE.parent / 'src-tauri/src/translation.py')
translation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(translation)

class TranslationTests(unittest.TestCase):
    def test_language_codes_cannot_escape_storage(self):
        for source in ('../en', 'C:/en', '', 'EN', 'en/../ja'):
            with self.assertRaises(ValueError):
                translation.pack_id(source, 'vi')

    def test_archive_traversal_is_rejected(self):
        data = io.BytesIO()
        with zipfile.ZipFile(data, 'w') as archive:
            archive.writestr('../outside.txt', 'no')
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(ValueError):
                translation.unzip(data.getvalue(), Path(folder))

    def test_missing_pack_does_not_download(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(translation, 'download', side_effect=AssertionError('Unexpected network')):
            with self.assertRaisesRegex(ValueError, 'Install vi'):
                translation.translate(Path(folder), {'source':'vi', 'target':'en', 'text':'Xin chào'})

    def test_same_language_keeps_original(self):
        self.assertEqual(translation.translate(BASE, {'source':'en', 'target':'en', 'text':'123_456\nHello'})['text'], '123_456\nHello')

    def test_real_japanese_offline(self):
        root = BASE / 'translation-runtime'
        with patch.object(translation, 'download', side_effect=AssertionError('Unexpected network')):
            value = translation.translate(root, {'source':'auto', 'target':'en', 'text':'日本語の言語検出をテストしています。\n数字 1234567890'})
        self.assertEqual(value['source'], 'ja')
        self.assertIn('Japanese', value['text'])
        self.assertIn('1234567890', value['text'])
        self.assertIn('\n', value['text'])
        self.assertNotIn('▁', value['text'])

    def test_real_vietnamese_offline(self):
        with patch.object(translation, 'download', side_effect=AssertionError('Unexpected network')):
            value = translation.translate(BASE / 'translation-runtime', {'source':'auto', 'target':'en', 'text':'Xin chào. Đây là bản dịch ngoại tuyến.\nGiá là 12345 đồng.'})
        self.assertEqual(value['source'], 'vi')
        self.assertIn('offline translation', value['text'])
        self.assertIn('12345', value['text'])
        self.assertNotIn('▁', value['text'])

    def test_japanese_to_vietnamese_bridge_offline(self):
        with patch.object(translation, 'download', side_effect=AssertionError('Unexpected network')):
            value = translation.translate(BASE / 'translation-runtime', {'source':'ja', 'target':'vi', 'text':'こんにちは。数字 1234567890'})
        self.assertIn('1234567890', value['text'])
        self.assertNotIn('▁', value['text'])

if __name__ == '__main__':
    unittest.main()
