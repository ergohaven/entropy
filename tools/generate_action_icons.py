#!/usr/bin/env python3
"""Generate 35px action icons shared by Entropy and macropad firmware.

Source: Lucide 0.468.0 static SVGs (ISC); the source tarball path is an argument.
The preset map is semantic, not inferred from ambiguous key combinations.
"""
import argparse
import io
import json
import tarfile
from pathlib import Path
from PIL import Image
import cairosvg

# Control order: 3x3 keys, then encoder press / CCW / CW. Empty = intentionally unbound.
MAP = {
    'google_chrome': [
        ('Browsing', 'link plus search arrow-left arrow-right history move-left move-right maximize link move-down move-up'),
        ('Pages', 'bookmark download history square-pen save printer focus zoom-out zoom-in search zoom-out zoom-in'),
    ],
    'visual_studio_code': [
        ('Coding', 'command folder-open terminal panel-left panel-bottom message-square-code bug bug-off text-cursor-input folder-open move-left move-right'),
        ('Search', 'search replace folder-search files git-branch list-filter mouse-pointer-2 move-up move-down search arrow-up arrow-down'),
        ('Files & terminal', 'file-plus folder-open save file-pen terminal square-terminal panel-left folder-search blocks save move-left move-right'),
    ],
    'adobe_photoshop': [
        ('Tools', 'mouse-pointer-2 paintbrush eraser crop lasso square-dashed pipette bandage type scan zoom-out zoom-in'),
        ('Editing', 'undo redo copy save mouse-pointer select-all copy paste focus scan zoom-out zoom-in'),
        ('Layers', 'layers-plus copy group blend eye component list-restart layers file-pen copy move-left move-right'),
    ],
    'adobe_premiere_pro': [
        ('Editing', 'mouse-pointer-2 scissors corner-up-left corner-up-right move-horizontal move-vertical hand zoom-in play play step-back step-forward'),
        ('Timeline', 'flag flag-triangle-right step-back pause step-forward bookmark scissors undo redo pause arrow-left arrow-right'),
        ('Project', 'save folder-open file-plus copy paste select-all skip-back skip-forward _ save zoom-in zoom-out'),
    ],
    'figma': [
        ('Design', 'mouse-pointer-2 frame rectangle-horizontal pen-tool type expand focus command undo command zoom-out zoom-in'),
        ('Arrange', 'circle line-squiggle hand scaling select-all copy paste copy-plus group ungroup zoom-out zoom-in'),
        ('Layers', 'ungroup component image-plus move-down move-up send-to-back bring-to-front scan-line text-cursor-input group move-down move-up'),
    ],
    'adobe_illustrator': [
        ('Tools', 'mouse-pointer-2 mouse-pointer pen-tool type rectangle-horizontal circle paintbrush pencil scissors group zoom-out zoom-in'),
        ('Objects', 'group ungroup copy paste undo redo select-all copy-plus layers group move-down move-up'),
        ('Documents', 'file-plus folder-open save file-pen printer search skip-back skip-forward _ save zoom-out zoom-in'),
    ],
    'blender': [
        ('Modeling', 'move rotate-cw scaling keyboard package-plus search blend undo redo focus zoom-out zoom-in'),
        ('Selection', 'select-all scan-dashed list-restart eye-off eye-off eye panel-left panel-right text-cursor-input focus zoom-out zoom-in'),
        ('Views', 'view-front view-right view-top box camera focus expand search panel-right focus rotate-ccw rotate-cw'),
    ],
    'visual_studio': [
        ('Editing', 'save search replace undo redo copy paste select-all text-cursor-input save zoom-in zoom-out'),
        ('Debugging', 'bug bug-off step-forward step-back arrow-right _ _ _ _ bug arrow-left arrow-right'),
        ('Files', 'file-plus folder-open file-pen list-filter command _ _ _ _ command move-down move-up'),
    ],
    'intellij_idea': [
        ('Coding', 'save search undo redo copy paste select-all corner-up-right _ save zoom-in zoom-out'),
        ('Navigation', 'search files replace list-filter command _ _ _ _ search move-down move-up'),
        ('Run & files', 'bug play file-plus folder-open file-pen _ _ _ _ save move-down move-up'),
    ],
    'pycharm': [
        ('Coding', 'save search undo redo copy paste select-all corner-up-right _ save zoom-in zoom-out'),
        ('Navigation', 'search files replace list-filter command _ _ _ _ search move-down move-up'),
        ('Run & files', 'bug play file-plus folder-open file-pen _ _ _ _ save move-down move-up'),
    ],
    'obs_studio': [
        ('Sources', 'undo redo search copy rotate-cw eye arrow-left arrow-up arrow-right eye arrow-down arrow-up'),
        ('Source order', 'copy paste move-up move-down bring-to-front send-to-back expand arrow-down focus expand move-down move-up'),
    ],
    'discord': [
        ('Navigation', 'route search circle-arrow-up circle-arrow-down arrow-down-to-line arrow-up-to-line _ _ _ route arrow-up arrow-down'),
        ('Voice', 'mic-off headphones _ _ _ _ _ _ _ _ volume-x volume-2'),
    ],
    'firefox': [
        ('Browsing', 'link plus search arrow-left arrow-right history maximize move-right move-left link move-down move-up'),
        ('Reading', 'bookmark download history incognito save code bookmark focus book-open focus zoom-out zoom-in'),
    ],
    'audacity': [
        ('Audio', 'play pause step-back pause step-forward skip-back skip-forward undo redo search zoom-out zoom-in'),
        ('Clip editing', 'scissors copy paste copy-plus select-all save file-plus folder-open file-output audio-lines move-down move-up'),
    ],
}

# A few semantic concepts are represented by a unique icon in Lucide; all
# command artwork is shared between firmware and Entropy editor.
RU = {
    'link':'Адрес','plus':'Новая вкладка','search':'Поиск','arrow-left':'Назад','arrow-right':'Вперёд',
    'history':'История','move-left':'Влево','move-right':'Вправо','maximize':'Полный экран',
    'move-down':'Сдвиг вниз','move-up':'Сдвиг вверх','bookmark':'Закладка','download':'Загрузки',
    'square-pen':'Новое окно','save':'Сохранить','printer':'Печать','focus':'По размеру',
    'zoom-out':'Уменьшить','zoom-in':'Увеличить','command':'Команды','folder-open':'Открыть',
    'terminal':'Терминал','panel-left':'Боковая панель','panel-bottom':'Нижняя панель',
    'message-square-code':'Комментарий','bug':'Отладка','bug-off':'Остановить отладку',
    'text-cursor-input':'Переименовать','replace':'Заменить','folder-search':'Поиск в файлах',
    'files':'Файлы','git-branch':'Контроль версий','list-filter':'Перейти',
    'move':'Перемещение','mouse-pointer-2':'Выделение','arrow-up':'Вверх','arrow-down':'Вниз',
    'file-plus':'Новый файл','file-pen':'Сохранить как','square-terminal':'Новый терминал',
    'blocks':'Расширения','paintbrush':'Кисть','eraser':'Ластик','crop':'Кадрировать',
    'lasso':'Лассо','square-dashed':'Область','pipette':'Пипетка','bandage':'Восстановить',
    'type':'Текст','scan':'Трансформация','undo':'Отменить','redo':'Повторить',
    'copy':'Копировать','mouse-pointer':'Указатель','select-all':'Выделить всё',
    'paste':'Вставить','layers-plus':'Новый слой','group':'Группа','blend':'Объединить',
    'eye':'Показать','component':'Компонент','list-restart':'Инвертировать',
    'layers':'Слои','scissors':'Разрезать','corner-up-left':'Монтаж влево',
    'corner-up-right':'Монтаж вправо','move-horizontal':'Сдвиг','move-vertical':'Сдвиг по вертикали',
    'hand':'Рука','play':'Воспроизвести','step-back':'Назад по кадрам',
    'step-forward':'Вперёд по кадрам','flag':'Метка начала',
    'flag-triangle-right':'Метка конца','pause':'Пауза','skip-back':'В начало',
    'skip-forward':'В конец','frame':'Фрейм','rectangle-horizontal':'Прямоугольник',
    'pen-tool':'Перо','expand':'Развернуть','circle':'Эллипс','line-squiggle':'Линия',
    'scaling':'Масштаб','copy-plus':'Дублировать','ungroup':'Разгруппировать',
    'image-plus':'Добавить изображение','send-to-back':'На задний план',
    'bring-to-front':'На передний план','scan-line':'Обводка','pencil':'Карандаш',
    'rotate-cw':'Повернуть','keyboard':'Режим','package-plus':'Добавить',
    'scan-dashed':'Сброс выделения','eye-off':'Скрыть','panel-right':'Правая панель',
    'view-front':'Спереди','view-right':'Справа','view-top':'Сверху','box':'Перспектива',
    'camera':'Камера','rotate-ccw':'Поворот назад','code':'Код',
    'mic-off':'Выключить микрофон','headphones':'Наушники','volume-x':'Тише',
    'volume-2':'Громче','incognito':'Приватное окно','book-open':'Закладки',
    'file-output':'Экспорт','audio-lines':'Звуковая дорожка',
    'route':'Быстрый переход','circle-arrow-up':'Канал выше','circle-arrow-down':'Канал ниже',
    'arrow-down-to-line':'Страница вниз','arrow-up-to-line':'Страница вверх'
}

ALIASES = {
    'select-all': 'grid-2x2-check',
    'paste': 'clipboard-paste',
    'layers-plus': 'grid-2x2-plus',
    'line-squiggle': 'slash',
    'scan-dashed': 'square-dashed-mouse-pointer',
    'view-front': 'square-arrow-down',
    'view-right': 'square-arrow-right',
    'view-top': 'square-arrow-up',
    'incognito': 'ghost',
}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('lucide_tar', type=Path)
    parser.add_argument('firmware_root', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    symbols = list(dict.fromkeys(icon for layers in MAP.values() for _, line in layers for icon in line.split() if icon != '_'))
    assert len(symbols) <= 244, len(symbols)
    assert len(RU.keys() & set(symbols)) == len(symbols), set(symbols) - RU.keys()
    with tarfile.open(args.lucide_tar, 'r:gz') as archive:
        available = set(archive.getnames())
        missing = [name for name in symbols if f'package/icons/{ALIASES.get(name, name)}.svg' not in available]
        assert not missing, missing
        icons = []
        for symbol in symbols:
            svg = archive.extractfile(f'package/icons/{ALIASES.get(symbol, symbol)}.svg').read()
            # Keep the 35px footprint, but reduce Lucide's heavy default stroke.
            svg = svg.replace(b'stroke-width="2"', b'stroke-width="1.7"')
            png = cairosvg.svg2png(bytestring=svg, output_width=35, output_height=35)
            rgba = Image.open(io.BytesIO(png)).convert('RGBA')
            bits = [rgba.getpixel((x, y))[3] >= 72 for y in range(35) for x in range(35)]
            assert any(bits), symbol
            packed = bytes(sum(0x80 >> j for j in range(8) if i + j < len(bits) and bits[i+j]) for i in range(0, len(bits), 8))
            assert len(packed) == 154
            icons.append(packed)
    assert len(set(icons)) == len(icons), 'duplicate icon artwork'
    (root / 'assets/action-icons.bin').write_bytes(b''.join(icons))
    catalog = {'symbols': symbols, 'presets': {name: [{'layer': layer, 'icons': line.split()} for layer,line in layers] for name,layers in MAP.items()}}
    (root / 'assets/action-icon-map.json').write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + '\n')
    lookups = {name: 12+i for i,name in enumerate(symbols)}
    lines = [
        '// Generated by tools/generate_action_icons.py. Do not edit by hand.',
        'pub(crate) const ACTION_ICON_COUNT: usize = %d;' % len(symbols),
        'pub(crate) const ACTION_ICON_KEYS: [&str; ACTION_ICON_COUNT] = [',
    ]
    lines += [f'    "display_settings.pictogram_builtin_action_{name.replace("-", "_")}",' for name in symbols]
    lines += ['];', 'pub(crate) fn bitmap(index: usize) -> [u8; 154] {',
              '    let bytes = include_bytes!("../assets/action-icons.bin");',
              '    let mut out = [0; 154];',
              '    let start = index * 154;',
              '    out.copy_from_slice(&bytes[start..start + 154]);',
              '    out', '}',
              'pub(crate) fn preset_visual(preset_id: &str, layer: usize, control: usize) -> u8 {',
              '    let icons: &[&[u8; 12]] = match preset_id {']
    for name,layers in MAP.items():
        lines.append(f'        "{name}" => &[')
        for layer, words in layers:
            words = words.split()
            assert len(words) == 12, (name, layer, len(words), words)
            lines.append('            &[' + ', '.join(str(lookups.get(word, 0)) for word in words) + '], // ' + layer)
        lines.append('        ],')
    lines += ['        _ => return 0,', '    };',
              '    let index = if control < 9 { control } else if (12..15).contains(&control) { control - 3 } else { return 0 };',
              '    icons.get(layer).map_or(0, |icons| icons[index])', '}']
    (root/'src/action_icons.rs').write_text('\n'.join(lines)+'\n')
    header = [
        '// Generated by Entropy tools/generate_action_icons.py from Lucide 0.468.0 (ISC).',
        '#pragma once', '#include <stdint.h>',
        f'#define EH_ACTION_ICON_COUNT {len(symbols)}',
        '#define EH_ACTION_ICON_FIRST_VISUAL 12',
        'static const uint8_t eh_action_icons[EH_ACTION_ICON_COUNT][154] = {',
    ]
    for symbol, bits in zip(symbols, icons):
        header.append('    { // ' + symbol)
        header += ['        ' + ', '.join(f'0x{byte:02x}' for byte in bits[i:i+16]) + ',' for i in range(0,154,16)]
        header.append('    },')
    header.append('};')
    (args.firmware_root/'keyboards/ergohaven/macropad/action_icons.h').write_text('\n'.join(header)+'\n')
    for language in ('en','ru'):
        path = root/'i18n'/f'{language}.toml'
        content = path.read_text()
        marker = '\n# Generated preset action pictograms\n'
        end_marker = '# End generated preset action pictograms\n'
        if marker in content:
            before, after = content.split(marker, 1)
            content = before + '\n' + after.split(end_marker, 1)[1].lstrip('\n')
        labels = [marker]
        for symbol in symbols:
            key = f'pictogram_builtin_action_{symbol.replace("-", "_")}'
            value = symbol.replace('-', ' ').title() if language == 'en' else RU[symbol]
            labels.append(f'{key} = {json.dumps(value, ensure_ascii=False)}\n')
        labels.append(end_marker)
        # These keys must remain within [display_settings], so insert directly
        # after the existing calculator key instead of at the end of the TOML.
        insertion = 'pictogram_builtin_calculator = '
        offset = content.index('\n', content.index(insertion))
        content = content[:offset] + '\n' + ''.join(labels).strip('\n') + content[offset:]
        path.write_text(content)
    (root/'assets/ACTION-ICON-SOURCES.txt').write_text(
        'Action icon artwork is adapted from Lucide 0.468.0 SVG icons.\n'
        'Copyright (c) for portions of Lucide are held by the Lucide Contributors 2022.\n'
        'License: ISC, https://github.com/lucide-icons/lucide/blob/main/LICENSE\n'
        'Generated by tools/generate_action_icons.py at 35x35 monochrome resolution.\n'
    )
    print(f'{len(symbols)} action icons, {len(MAP)} preset families, {sum(len(v) for v in MAP.values())} layers')

if __name__ == '__main__':
    main()
