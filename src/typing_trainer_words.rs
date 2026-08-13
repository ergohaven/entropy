#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TypingTrainerLanguage {
    #[default]
    English,
    Russian,
}

pub(crate) const TYPING_TRAINER_LANGUAGES: [TypingTrainerLanguage; 2] = [
    TypingTrainerLanguage::English,
    TypingTrainerLanguage::Russian,
];

impl TypingTrainerLanguage {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Russian => "ru",
        }
    }
}

/// The trainer language doubles as the input mapping of the exercise: it tells
/// which OS layout the typed characters are expected to come from.
impl From<TypingTrainerLanguage> for crate::keycode::KeyOutputLayout {
    fn from(language: TypingTrainerLanguage) -> Self {
        match language {
            TypingTrainerLanguage::English => Self::English,
            TypingTrainerLanguage::Russian => Self::Russian,
        }
    }
}

pub(crate) fn word_count(language: TypingTrainerLanguage) -> usize {
    words(language).split_whitespace().count()
}

pub(crate) fn word_at(language: TypingTrainerLanguage, idx: usize) -> &'static str {
    words(language)
        .split_whitespace()
        .nth(idx)
        .unwrap_or_else(|| words(language).split_whitespace().next().unwrap_or("word"))
}

fn words(language: TypingTrainerLanguage) -> &'static str {
    match language {
        TypingTrainerLanguage::English => ENGLISH_WORDS,
        TypingTrainerLanguage::Russian => RUSSIAN_WORDS,
    }
}

const ENGLISH_WORDS: &str = "\
about above accept across action active actual add address admit after again air allow almost alone along already also always amount animal answer anyone appear apply area argue arm around arrive article artist assume attack audio author avoid away baby back balance bank base basic battle beauty become bed before begin behavior behind believe benefit best better between beyond big bill bird black blue board body book born both box boy branch break bridge bright bring brother budget build business busy call camera campaign can capital car card care carry case catch cause center chance change charge check child choice choose church city claim class clean clear close cloud coach code cold college color come common company complete condition control copy corner cost could country course cover create culture current customer cut dark data daughter day deal death decide deep degree design detail device develop different difficult direction discover door down draw drive during each early earth east easy edge education effect effort either else employee end energy enjoy enough enter even every example exist expect explain eye face fact fail fall family far fast father fear feature feel field fight figure file fill final find fine finish fire firm first floor focus follow food foot force form forward found free friend front full future game garden general get girl give goal good government great green ground group grow guess guide hand hard head hear heart heat help here high hold home hope hour house human idea image important improve include increase industry input inside interest into issue item join just keep key keyboard kind know language large last late later layer learn leave left less letter level life light line list little live local long look lose low machine main make man manage many map mark market matrix matter may mean measure media meet member method middle might mind minute miss modern money month more morning most mother move much music name natural near need never new next night normal north note number object offer office often once open order other owner page paper parent part party pass people period person picture place plan point policy position possible power press price problem process produce product program project public pull quality question quick quite radio raise range rate reach read ready real reason receive record reduce relate release remain remember report result return rich right room rule run same save say school science sea season second section see seem send sentence service set several shape share short should show side simple since site size small social some sound south space speak special speed spend stand start state stay step still stop story street strong student study style subject success such support sure system table take talk task team tell term test than their them then there these thing think third through time today together too tool top total town trade train travel tree true try turn type under understand unit until update upon use user value very view visit voice wait walk wall want war warm watch water way week well went were west what when where while white whole why wide window without woman word work world write year yes young";

const RUSSIAN_WORDS: &str = "\
автор август адрес актив акт анализ аптека армия база баланс банк берег билет бизнес близко блок боль большой брат буква бумага быстро важный вариант вечер вещь вид видеть внимание вода вопрос время все всегда встреча выбор выйти где герой глава глаз голос город готовый группа дать дело день деньги держать деталь дети делать дерево дом дорога друг думать душа есть ехать женщина жизнь журнал задача закон закрыть зал запись запрос знать значение игра идея идти имя иметь именно искать история карта качество книга кнопка код команда комната конец контроль корпус красивый круг крупный кто культура левый легко лес лето линия лист лицо лучший люди маленький мама машина место месяц метод минута мир много может момент море Москва мысль надо назад найти народ начало наш небо небольшой неделя нельзя новый ночь нужно объект окно опыт ответ открыть отец очень память папа параметр парк партия первый период письмо план плохо площадь под поле полный помощь понять порядок после потом почему право правило пример проблема программа проект простой процесс путь работа раз разный район рано ребенок решение результат речь рука рынок сад самый свет свой сделать сейчас семья система сказать слой случай слово смотреть снова событие совсем создать сторона строка студент сюда таблица также там тема теперь текст телефон тело тест тип тогда точка три тут увидеть уже узнать улица уровень урок условие утро файл факт форма хороший хотя хотеть цвет центр цена человек через число чистый школа экран элемент язык явный адресат аппарат аренда архив атмосфера безопасность белый быстро вагон валюта верх ветер вечный вкус вклад внутри воздух волна восток вход газ газета гарантия герой глубина год голова голосование гора готовность граница далеко двор действие директор документ доля доступ доход единый железо завод занятие запас защита звук земля знак зона изделие инструмент итог кадр канал квартира класс клиент клуб ключ коллектив количество комиссия конкурс контакт корабль лесной магазин материал мера металл модель мост настройка наука номер образ огонь отдел память победа поезд поиск поток причина ресурс робот роль сила скорость совет список срок сумма товар труд факт фильм фонд ход цель часть шаг шанс";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_trainer_language_packs_are_large_enough() {
        assert_eq!(
            TypingTrainerLanguage::default(),
            TypingTrainerLanguage::English
        );
        for language in TYPING_TRAINER_LANGUAGES {
            assert!(word_count(language) >= 300);
        }
    }
}
