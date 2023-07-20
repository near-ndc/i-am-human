struct Poll{
    verified_humans_only: bool, // required, if true only verified humans can vote, if false anyone can vote
    questions: Vec<PollQuestion>, // required, a poll can have any number of questions
    starts_at: usize, // required, time in milliseconds
    end_at: usize, // required, time in milliseconds
      title: String, // required
    tags: Vec<String>, // can be an empty vector
    description: Option<String>, // optional
    link: Option<String>, // optional
      created_at: usize, // should be assigned by the smart contract not the user, time in milliseconds
  }
  struct PollQuestion{
    question_type: PollQuestionType, // required
    required: bool, // required, if true users can't vote without having an answer for this question
    title: String, // required
    description: Option<String>, // optional
    image: Option<String>, // optional
      labels: Option<(String, String, String)>, // if applicable, labels for the opinion scale question
      choices: Option<Vec<usize>>, // if applicable, choices for the text and picture choices question
  }
  struct PollResult {
    status: not_started | active | finished,
    results: Vec<(usize, Vec<PollQuestionAnswer>)>, // question_id, list of answers
    number_of_participants: u64,
  }
  
  enum PollQuestionType {
    YesNo,
    TextChoices(min_choices, max_choices),
    PictureChoices(min_choices, max_choices), 
    OpinionScale,
    TextAnswer,
  }
  
  // user can update the poll if starts_at > now
  // it panics if 
  // - user tries to create an invalid poll
  // - if poll aready exists and starts_at < now
  fn create_poll(new_poll: Poll) -> PollId;
  
  struct Vote {
    answers: Vec<(usize, PollQuestionAnswer)>, // question_id, answer
      created_at: usize, // should be assigned by the smart contract not the user, time in milliseconds
  }
  
  enum PollQuestionAnswer {
    YesNo(bool),
    TextChoices(Vec<String>), // should respect the min_choices, max_choices
    PictureChoices(Vec<String>), // should respect the min_choices, max_choices
    OpinionScale(usize), // should be a number between 0 and 10
    TextAnswer(String),
  }
  
  // user can change his vote when the poll is still active.
  // it panics if 
  // - poll not found
  // - poll not active
  // - poll.verified_humans_only is true, and user is not verified on IAH
  // - user tries to vote with an invalid answer to a question
  fn vote(poll_id: PollId, vote: Vote);
  
  // returns None if poll is not found
  fn result(poll_id: usize) -> Option<PollResult>;
  